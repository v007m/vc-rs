//! A load run while it runs: each closed second to the history and the live view, failed
//! iterations counted by kind, and Ctrl-C passed on.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};
use tokio::sync::mpsc;
use vc_executor::{Arguments, CancelTree, IterationReport, MatcherRegistry, ProgramIterations};
use vc_interfaces::{IntervalRow, Random, Row};
use vc_loadtest::{Breach, Failed, LoadError, LoadOutcome, LoadParts, Second, run_load};
use vc_model::{ActionId, ApiId, Event, Exchange, LoadScenario, Part, Program, TemplateValue, TestId};
use vc_template::RenderError;

use crate::history::Recording;
use crate::{ErrorKind, Interrupts};

/// What failed an iteration: the api its failing action calls, the test, that action's last
/// status and the failure's code.
type Kind = (Option<ApiId>, Option<TestId>, Option<u16>, Option<String>);

/// A failed iteration's exchange, unmasked until the run closes, and the action that sent it.
pub type Sample = (Option<ActionId>, Box<Exchange>);

/// How many exchanges of each error kind the history keeps (D326).
const SAMPLES: usize = 5;

/// How often a kind of error happened, and its first [`SAMPLES`] exchanges.
#[derive(Debug, Default)]
struct Tally {
    count: u64,
    samples: Vec<Sample>,
}

/// Closed seconds the stage loop may queue before it waits for the history.
const SECONDS_QUEUE: usize = 16;

/// What a load run keeps while it runs, owned by the task that follows its seconds.
pub struct Live {
    program: Arc<Program>,
    /// The history's run, until it is closed.
    pub recording: Option<Recording>,
    ticks: Option<mpsc::Sender<Event>>,
    kinds: BTreeMap<Kind, Tally>,
    /// The first rows the history could not take.
    pub problem: Option<String>,
}

impl Live {
    pub const fn new(program: Arc<Program>, recording: Option<Recording>, ticks: Option<mpsc::Sender<Event>>) -> Self {
        Self { program, recording, ticks, kinds: BTreeMap::new(), problem: None }
    }

    /// The history's number for the run, when it is recorded.
    pub fn run_id(&self) -> Option<u64> {
        self.recording.as_ref().map(|recording| recording.id().get())
    }

    /// Takes the failed iterations by kind, most first, each with its first few exchanges.
    pub fn error_kinds(&mut self, program: &Program) -> Vec<(ErrorKind, Vec<Sample>)> {
        let mut kinds: Vec<(ErrorKind, Vec<Sample>)> = std::mem::take(&mut self.kinds)
            .into_iter()
            .map(|((api, test, status, code), tally)| {
                let api = api.and_then(|api| program.str(api.sym())).map(str::to_owned);
                let test = test.and_then(|test| program.str(test.sym())).map(str::to_owned);
                (ErrorKind { api, test, status, code, count: tally.count }, tally.samples)
            })
            .collect();
        kinds.sort_by_key(|(kind, _)| std::cmp::Reverse(kind.count));
        kinds
    }

    /// Writes each second as it closes and sends its tick, until the stage loop is done.
    async fn follow(
        mut self,
        mut seconds: mpsc::Receiver<Second>,
        mut failed: mpsc::UnboundedReceiver<Failed>,
    ) -> Self {
        while let Some(second) = seconds.recv().await {
            self.count(&mut failed);
            self.second(&second);
        }
        self.count(&mut failed);
        self
    }

    /// Counts by kind the failed iterations the run has kept so far, keeping the exchanges of
    /// each kind's first few.
    fn count(&mut self, failed: &mut mpsc::UnboundedReceiver<Failed>) {
        while let Ok((test, mut report)) = failed.try_recv() {
            let tally = self.kinds.entry(kind(test, &report)).or_default();
            tally.count = tally.count.saturating_add(1);
            if tally.samples.len() < SAMPLES
                && let Some(failure) = report.failure.as_mut()
                && let Some(exchange) = failure.exchange.take()
            {
                tally.samples.push((failure.action, exchange));
            }
        }
    }

    fn second(&mut self, second: &Second) {
        let rows = second.rows.iter().map(|interval| Row::Interval(self.row(interval))).collect();
        if let Some(recording) = &self.recording
            && let Err(error) = recording.record(rows)
            && self.problem.is_none()
        {
            self.problem = Some(format!("history: {error}; the run's seconds are not all recorded"));
        }
        if let Some(ticks) = &self.ticks {
            let _dropped = ticks.try_send(tick(&self.program, second));
        }
    }

    fn row(&self, interval: &vc_loadtest::Interval) -> IntervalRow {
        IntervalRow {
            test_id: self.program.str(interval.test.sym()).unwrap_or_default().to_owned(),
            second: interval.second,
            count: interval.tally.count(),
            errors: interval.tally.errors(),
            histogram: interval.tally.latencies().encode(),
        }
    }
}

/// The live view's line for `second`: its rate over the last few seconds, the rest over the
/// rolling window.
fn tick(program: &Program, second: &Second) -> Event {
    let all = second.window.all();
    let latencies = all.latencies();
    Event::LoadTick {
        second: second.at,
        users: second.users,
        rps: second.rps,
        p50: millis(latencies.quantile_ms(0.5)),
        p90: millis(latencies.quantile_ms(0.9)),
        p95: millis(latencies.quantile_ms(0.95)),
        p99: millis(latencies.quantile_ms(0.99)),
        errors: all.errors(),
        breaching: second.breaching.iter().map(|breach| breach_name(program, breach)).collect(),
    }
}

fn millis(ms: f64) -> Duration {
    Duration::try_from_secs_f64(ms / 1000.0).unwrap_or_default()
}

/// `p95`, or `<test>: p95` for a test's own threshold.
fn breach_name(program: &Program, breach: &Breach) -> String {
    let test = breach.test.and_then(|test| program.str(test.sym()));
    test.map_or_else(|| breach.metric.clone(), |test| format!("{test}: {}", breach.metric))
}

/// The kind of `test`'s failed iteration: by the test and the action that failed it, not by
/// the calls before or after (a `before` call, a cleanup).
fn kind(test: TestId, report: &IterationReport) -> Kind {
    let Some(failure) = &report.failure else { return (None, Some(test), None, None) };
    (failure.api, Some(test), failure.status, failure.code.clone())
}

/// Renders a threshold's literal value; a load run has no call for a reference to read.
struct Literal;

impl Arguments for Literal {
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        literal(value).ok_or_else(|| RenderError::invalid("a load threshold takes a literal value, not a reference"))
    }
}

fn literal(value: &TemplateValue) -> Option<Value> {
    match value {
        TemplateValue::Null => Some(Value::Null),
        TemplateValue::Bool(flag) => Some(Value::Bool(*flag)),
        TemplateValue::Number(number) => Some(Value::Number(number.clone())),
        TemplateValue::String(template) => template
            .parts
            .iter()
            .map(|part| if let Part::Literal(text) = part { Some(text.as_str()) } else { None })
            .collect::<Option<String>>()
            .map(Value::String),
        TemplateValue::List(items) => items.iter().map(literal).collect::<Option<Vec<_>>>().map(Value::Array),
        TemplateValue::Map(fields) => fields
            .iter()
            .map(|(name, field)| literal(field).map(|field| (name.clone(), field)))
            .collect::<Option<Map<_, _>>>()
            .map(Value::Object),
        _ => None,
    }
}

/// Runs `scenario`'s stages over `iterations` while `live` follows its seconds, then runs
/// `afterAll`, `afterRun` and the shared cleanups.
pub async fn measure(
    scenario: &LoadScenario,
    (iterations, live): (Arc<ProgramIterations>, Live),
    random: &Arc<dyn Random>,
) -> (Result<LoadOutcome, LoadError>, Live) {
    let (seconds, closed) = mpsc::channel(SECONDS_QUEUE);
    let (failed, kept) = mpsc::unbounded_channel();
    let (shared, cancel) = (Arc::clone(&iterations), iterations.users());
    let parts = LoadParts { iterations: shared, random: Arc::clone(random), cancel, seconds, failed };
    let registry = MatcherRegistry::standard();
    let (outcome, live) = tokio::join!(run_load(scenario, parts, (&registry, &Literal)), live.follow(closed, kept));
    iterations.finish().await;
    (outcome, live)
}

/// Awaits `work`, passing each Ctrl-C on to `root`: the first cancels it, the second skips
/// the grace period, the third abandons the cleanups.
pub async fn forwarding<T>(work: impl Future<Output = T>, interrupts: &mut Interrupts, root: &CancelTree) -> T {
    tokio::pin!(work);
    loop {
        tokio::select! {
            biased;
            done = &mut work => return done,
            Some(()) = interrupts.next() => match interrupts.seen() {
                1 => root.cancel(),
                2 => root.skip_grace(),
                _ => root.abort(),
            },
        }
    }
}

#[cfg(test)]
#[path = "load_live_tests.rs"]
mod tests;
