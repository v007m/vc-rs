use std::collections::BTreeMap;
use std::sync::mpsc::{self, Sender};

use vc_interfaces::{
    ApiTrend, ExchangeRow, HistoryError, HistoryReader, HistoryWriter, Row, RunEnd, RunFilter, RunId, RunRef, RunRow,
    RunStart, TestRow, TestTrend, api_trends,
};
use vc_model::{Keep, TestStatus, Totals};

/// A run history held in memory.
///
/// A small thread owns it, so the writer and the reader share it without a lock. Clones talk to
/// the same thread, so a test can hand one to the engine as its writer and keep another to read
/// what it stored. Unlike the SQLite writer it stores rows before `record` returns.
#[derive(Clone, Debug)]
pub struct MemoryHistory {
    requests: Sender<Request>,
}

#[derive(Debug)]
enum Request {
    Begin(RunStart, Sender<RunId>),
    Record(RunId, Vec<Row>, Sender<()>),
    Finish(RunId, RunEnd, Sender<()>),
    Prune(Keep, Sender<usize>),
    Runs(usize, RunFilter, Sender<Vec<RunRow>>),
    Run(RunRef, Sender<Option<RunRow>>),
    Tests(RunId, Sender<Vec<TestRow>>),
    Snapshot(RunId, Sender<Option<Vec<u8>>>),
    Trends(usize, RunFilter, Sender<Vec<TestTrend>>),
    ApiTrends(usize, RunFilter, Sender<Vec<ApiTrend>>),
}

/// One stored run.
#[derive(Debug)]
struct Stored {
    row: RunRow,
    tests: Vec<TestRow>,
    /// Every exchange, its tests' and lifecycle ones, in the order they were recorded.
    exchanges: Vec<ExchangeRow>,
    snapshot: Option<Vec<u8>>,
}

/// Every stored run, oldest first.
#[derive(Debug, Default)]
struct State {
    runs: Vec<Stored>,
    next: u64,
}

impl State {
    fn handle(&mut self, request: Request) {
        match request {
            Request::Begin(start, reply) => drop(reply.send(self.begin(start))),
            Request::Record(run, rows, reply) => {
                self.record(run, rows);
                reply.send(()).ok();
            }
            Request::Finish(run, end, reply) => {
                self.finish(run, end);
                reply.send(()).ok();
            }
            Request::Prune(keep, reply) => drop(reply.send(self.prune(keep))),
            Request::Runs(limit, filter, reply) => {
                let matching = self.runs.iter().rev().filter(|run| filter.matches(&run.row.start));
                drop(reply.send(matching.take(limit).map(|run| run.row.clone()).collect()));
            }
            Request::Run(which, reply) => drop(reply.send(self.find(which).map(|run| run.row.clone()))),
            Request::Tests(run, reply) => drop(reply.send(self.tests(run))),
            Request::Snapshot(run, reply) => {
                drop(reply.send(self.find(RunRef::Id(run)).and_then(|run| run.snapshot.clone())));
            }
            Request::Trends(window, filter, reply) => drop(reply.send(self.trends(window, filter))),
            Request::ApiTrends(window, filter, reply) => {
                let runs = self.window(window, filter);
                let calls: Vec<ExchangeRow> = runs.flat_map(|stored| stored.exchanges.iter().rev().cloned()).collect();
                drop(reply.send(api_trends(&calls)));
            }
        }
    }

    fn begin(&mut self, start: RunStart) -> RunId {
        self.next += 1;
        let row =
            RunRow { id: RunId::new(self.next), start, finished_ms: None, status: None, totals: Totals::default() };
        self.runs.push(Stored { row, tests: Vec::new(), exchanges: Vec::new(), snapshot: None });
        RunId::new(self.next)
    }

    fn record(&mut self, run: RunId, rows: Vec<Row>) {
        let Some(stored) = self.runs.iter_mut().find(|stored| stored.row.id == run) else { return };
        for row in rows {
            match row {
                Row::Test(test) => {
                    stored.exchanges.extend(test.exchanges.iter().cloned());
                    stored.tests.push(test);
                }
                Row::Exchange(exchange) => stored.exchanges.push(exchange),
                Row::Interval(_) => {}
            }
        }
    }

    fn finish(&mut self, run: RunId, end: RunEnd) {
        if let Some(stored) = self.runs.iter_mut().find(|stored| stored.row.id == run) {
            stored.row.finished_ms = Some(end.finished_ms);
            stored.row.status = Some(end.status);
            stored.row.totals = end.totals;
            stored.snapshot = Some(end.snapshot);
        }
    }

    fn prune(&mut self, keep: Keep) -> usize {
        let Keep::Runs(kept) = keep else { return 0 };
        let gone = self.runs.len().saturating_sub(usize::try_from(kept).unwrap_or(usize::MAX));
        self.runs.drain(..gone);
        gone
    }

    fn find(&self, which: RunRef) -> Option<&Stored> {
        match which {
            RunRef::Id(id) => self.runs.iter().find(|stored| stored.row.id == id),
            RunRef::Last => self.runs.iter().rev().find(|stored| stored.row.start.kind != "adhoc"),
        }
    }

    fn tests(&self, run: RunId) -> Vec<TestRow> {
        let tests = self.find(RunRef::Id(run)).map(|stored| stored.tests.clone()).unwrap_or_default();
        tests.into_iter().map(|test| TestRow { exchanges: Vec::new(), ..test }).collect()
    }

    /// The newest `window` runs `filter` matches, of kind `run` when it names none, newest first.
    fn window(&self, window: usize, filter: RunFilter) -> impl Iterator<Item = &Stored> {
        let filter = RunFilter { kind: Some(filter.kind.unwrap_or_else(|| "run".to_owned())), ..filter };
        self.runs.iter().rev().filter(move |stored| filter.matches(&stored.row.start)).take(window)
    }

    /// Each test's trend over the [`State::window`], its mean summed first.
    fn trends(&self, window: usize, filter: RunFilter) -> Vec<TestTrend> {
        let mut trends: BTreeMap<&str, TestTrend> = BTreeMap::new();
        let runs = self.window(window, filter);
        let ran = runs.flat_map(|stored| &stored.tests).filter(|test| test.status != Some(TestStatus::Skipped));
        for test in ran {
            let trend = trends
                .entry(test.test_id.as_str())
                .or_insert_with(|| TestTrend { test_id: test.test_id.clone(), ..TestTrend::default() });
            trend.results += 1;
            trend.failed += u32::from(matches!(test.status, Some(TestStatus::Failed | TestStatus::Blocked)));
            trend.flaky += u32::from(test.status == Some(TestStatus::Flaky));
            trend.mean_ms += test.duration_ms;
        }
        trends
            .into_values()
            .map(|trend| TestTrend { mean_ms: trend.mean_ms / f64::from(trend.results), ..trend })
            .collect()
    }
}

impl MemoryHistory {
    /// An empty history.
    #[must_use]
    pub fn new() -> Self {
        let (requests, inbox) = mpsc::channel();
        std::thread::spawn(move || {
            let mut state = State::default();
            for request in inbox {
                state.handle(request);
            }
        });
        Self { requests }
    }

    /// Sends the request `ask` builds and waits for its answer.
    fn ask<T>(&self, ask: impl FnOnce(Sender<T>) -> Request) -> Result<T, HistoryError> {
        let (reply, answer) = mpsc::channel();
        self.requests.send(ask(reply)).map_err(|_| HistoryError::Stopped)?;
        answer.recv().map_err(|_| HistoryError::Stopped)
    }
}

impl Default for MemoryHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryWriter for MemoryHistory {
    fn prune(&self, keep: Keep) -> Result<usize, HistoryError> {
        self.ask(|reply| Request::Prune(keep, reply))
    }

    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.ask(|reply| Request::Begin(start.clone(), reply))
    }

    fn record(&self, run: RunId, rows: Vec<Row>) -> Result<(), HistoryError> {
        self.ask(|reply| Request::Record(run, rows, reply))
    }

    fn finish(&self, run: RunId, end: RunEnd) -> Result<(), HistoryError> {
        self.ask(|reply| Request::Finish(run, end, reply))
    }
}

impl HistoryReader for MemoryHistory {
    fn runs(&self, limit: usize, filter: &RunFilter) -> Result<Vec<RunRow>, HistoryError> {
        self.ask(|reply| Request::Runs(limit, filter.clone(), reply))
    }

    fn run(&self, which: RunRef) -> Result<Option<RunRow>, HistoryError> {
        self.ask(|reply| Request::Run(which, reply))
    }

    fn tests(&self, run: RunId) -> Result<Vec<TestRow>, HistoryError> {
        self.ask(|reply| Request::Tests(run, reply))
    }

    fn snapshot(&self, run: RunId) -> Result<Option<Vec<u8>>, HistoryError> {
        self.ask(|reply| Request::Snapshot(run, reply))
    }

    fn trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<TestTrend>, HistoryError> {
        self.ask(|reply| Request::Trends(window, filter.clone(), reply))
    }

    fn api_trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<ApiTrend>, HistoryError> {
        self.ask(|reply| Request::ApiTrends(window, filter.clone(), reply))
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryHistory;
    use crate::{history_reader_suite, history_writer_suite};

    #[test]
    fn passes_the_history_writer_suite() {
        history_writer_suite(&mut || {
            let history = MemoryHistory::new();
            (Box::new(history.clone()), Box::new(history))
        });
    }

    #[test]
    fn passes_the_history_reader_suite() {
        history_reader_suite(&mut || {
            let history = MemoryHistory::new();
            (Box::new(history.clone()), Box::new(history))
        });
    }
}
