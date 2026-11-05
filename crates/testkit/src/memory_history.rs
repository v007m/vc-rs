use std::sync::mpsc::{self, Sender};

use vc_interfaces::{
    HistoryError, HistoryReader, HistoryWriter, Row, RunEnd, RunId, RunRef, RunRow, RunStart, TestRow,
};
use vc_model::{Keep, Totals};

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
    Runs(usize, Sender<Vec<RunRow>>),
    Run(RunRef, Sender<Option<RunRow>>),
    Tests(RunId, Sender<Vec<TestRow>>),
    Snapshot(RunId, Sender<Option<Vec<u8>>>),
}

/// One stored run.
#[derive(Debug)]
struct Stored {
    row: RunRow,
    tests: Vec<TestRow>,
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
            Request::Runs(limit, reply) => {
                drop(reply.send(self.runs.iter().rev().take(limit).map(|run| run.row.clone()).collect()));
            }
            Request::Run(which, reply) => drop(reply.send(self.find(which).map(|run| run.row.clone()))),
            Request::Tests(run, reply) => drop(reply.send(self.tests(run))),
            Request::Snapshot(run, reply) => {
                drop(reply.send(self.find(RunRef::Id(run)).and_then(|run| run.snapshot.clone())));
            }
        }
    }

    fn begin(&mut self, start: RunStart) -> RunId {
        self.next += 1;
        let row =
            RunRow { id: RunId::new(self.next), start, finished_ms: None, status: None, totals: Totals::default() };
        self.runs.push(Stored { row, tests: Vec::new(), snapshot: None });
        RunId::new(self.next)
    }

    fn record(&mut self, run: RunId, rows: Vec<Row>) {
        if let Some(stored) = self.runs.iter_mut().find(|stored| stored.row.id == run) {
            stored
                .tests
                .extend(rows.into_iter().filter_map(|row| if let Row::Test(test) = row { Some(test) } else { None }));
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
            RunRef::Last => self.runs.last(),
        }
    }

    fn tests(&self, run: RunId) -> Vec<TestRow> {
        let tests = self.find(RunRef::Id(run)).map(|stored| stored.tests.clone()).unwrap_or_default();
        tests.into_iter().map(|test| TestRow { exchanges: Vec::new(), ..test }).collect()
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
    fn runs(&self, limit: usize) -> Result<Vec<RunRow>, HistoryError> {
        self.ask(|reply| Request::Runs(limit, reply))
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
