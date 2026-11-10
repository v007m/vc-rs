//! A load run while it runs: each closed second to the history and the live view, failed
//! iterations counted by kind, and Ctrl-C passed on.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value};
use tokio::sync::mpsc;
use vc_executor::{Arguments, CancelTree, IterationReport, Iterations, MatcherRegistry, ProgramIterations};
use vc_interfaces::{IntervalRow, Random, Row};
use vc_loadtest::{Breach, LoadError, LoadOutcome, LoadParts, Second, run_load};
use vc_model::{ApiId, Event, LoadScenario, Part, Program, TemplateValue, TestId};
use vc_template::RenderError;

use crate::history::Recording;
use crate::{ErrorKind, Interrupts};

/// The api and status of the call that failed an iteration.
type Kind = (Option<ApiId>, Option<u16>);

/// Closed seconds the stage loop may queue before it waits for the history.
const SECONDS_QUEUE: usize = 16;

/// What a load run keeps while it runs, owned by the task that follows its seconds.
pub struct Live {
    program: Arc<Program>,
    /// The history's run, until it is closed.
    pub recording: Option<Recording>,
    ticks: Option<mpsc::Sender<Event>>,
    kinds: BTreeMap<Kind, u64>,
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

    /// The failed iterations by kind, most first.
    pub fn error_kinds(&self, program: &Program) -> Vec<ErrorKind> {
        let mut kinds: Vec<ErrorKind> = self
            .kinds
            .iter()
            .map(|(&(api, status), &count)| {
                let api = api.and_then(|api| program.str(api.sym())).map(str::to_owned);
                ErrorKind { api, status, count }
            })
            .collect();
        kinds.sort_by_key(|kind| std::cmp::Reverse(kind.count));
        kinds
    }

    /// Writes each second as it closes and sends its tick, until the stage loop is done.
    async fn follow(mut self, mut seconds: mpsc::Receiver<Second>, mut kinds: mpsc::UnboundedReceiver<Kind>) -> Self {
        while let Some(second) = seconds.recv().await {
            self.count(&mut kinds);
            self.second(&second);
        }
        self.count(&mut kinds);
        self
    }

    fn count(&mut self, kinds: &mut mpsc::UnboundedReceiver<Kind>) {
        while let Ok(kind) = kinds.try_recv() {
            let count = self.kinds.entry(kind).or_default();
            *count = count.saturating_add(1);
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

/// The live view's line for `second`: its numbers over the rolling window.
fn tick(program: &Program, second: &Second) -> Event {
    let all = second.window.all();
    let latencies = all.latencies();
    let span = second.window.seconds().max(1);
    Event::LoadTick {
        second: second.at,
        users: second.users,
        rps: all.count().saturating_add(span / 2) / span,
        p50: millis(latencies.quantile_ms(0.5)),
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

/// The program's iterations, each failure sent on as its kind.
struct Counted {
    inner: Arc<ProgramIterations>,
    errors: mpsc::UnboundedSender<Kind>,
}

#[async_trait]
impl Iterations for Counted {
    async fn iterate(&self, test: TestId, user: u32) -> IterationReport {
        let report = self.inner.iterate(test, user).await;
        if !report.passed {
            let _gone = self.errors.send(kind(&report));
        }
        report
    }
}

/// The first call with no response or an error status; else the last call.
fn kind(report: &IterationReport) -> Kind {
    let failed = report.calls.iter().find(|call| call.status.is_none_or(|status| status >= 400));
    failed.or_else(|| report.calls.last()).map_or((None, None), |call| (call.api, call.status))
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
    let (errors, kinds) = mpsc::unbounded_channel();
    let counted = Counted { inner: Arc::clone(&iterations), errors };
    let cancel = iterations.users();
    let parts = LoadParts { iterations: Arc::new(counted), random: Arc::clone(random), cancel, seconds };
    let registry = MatcherRegistry::standard();
    let (outcome, live) = tokio::join!(run_load(scenario, parts, (&registry, &Literal)), live.follow(closed, kinds));
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
