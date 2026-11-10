//! What `vc load` asks for, watches and reports.

use serde_json::{Value, json};
use tokio::sync::mpsc;
use vc_loadtest::{Breach, Ending, LoadOutcome};
use vc_model::{Event, Program, RunStatus};

use crate::Interrupts;
use crate::load::exit;

/// What `vc load <scenario>` runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct LoadRequest {
    /// The scenario's ID: its load file's ID, such as `checkout.peak`.
    pub scenario: String,
    /// `--profile`, in order.
    pub profiles: Vec<String>,
    /// `--set key=value`, in order.
    pub sets: Vec<String>,
}

/// What a load run hears and tells while it runs.
#[derive(Debug)]
#[expect(clippy::exhaustive_structs, reason = "built by the apps; every user is in this repo (D160)")]
pub struct LoadWatch {
    /// Each value is one Ctrl-C: the first stops the users, the second skips the grace period,
    /// the third abandons the cleanups.
    pub interrupts: Interrupts,
    /// Where each closed second goes, as an [`Event::LoadTick`]; a tick that finds the channel
    /// full is dropped, so a slow view never slows the run.
    pub ticks: Option<mpsc::Sender<Event>>,
}

/// How a load run ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadEnding {
    /// Every stage ran.
    #[default]
    Finished,
    /// An `abortOn` threshold breached on the rolling window.
    Aborted,
    /// Ctrl-C.
    Cancelled,
    /// A `beforeRun` or `beforeAll` action failed, so no user started.
    Blocked,
}

impl LoadEnding {
    /// `finished`, `aborted`, `cancelled` or `blocked`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Finished => "finished",
            Self::Aborted => "aborted",
            Self::Cancelled => "cancelled",
            Self::Blocked => "blocked",
        }
    }
}

/// A threshold the whole run breached.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct LoadBreach {
    /// The metric: `p95`, `errorRate`, `rps`, …
    pub metric: String,
    /// The test whose own threshold it is, or `None` for the scenario's.
    pub test: Option<String>,
    /// The value the run measured, in the metric's unit (ms for latencies).
    pub actual: f64,
}

/// Failed iterations of one kind: the api and status of the call that failed them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ErrorKind {
    /// The api called, or `None` for an inline request or an iteration that sent none.
    pub api: Option<String>,
    /// The response status, or `None` when no response came.
    pub status: Option<u16>,
    /// How many iterations failed so.
    pub count: u64,
}

/// What a load run did, over the whole run.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct LoadReport {
    /// The exit code (D127): 0 within thresholds, 1 blocked, 3 a threshold breached, 130
    /// cancelled.
    pub exit: u8,
    /// The scenario's ID.
    pub scenario: String,
    /// How it ended.
    pub ending: LoadEnding,
    /// The seconds it ran.
    pub seconds: u64,
    /// Iterations recorded.
    pub iterations: u64,
    /// Iterations that failed.
    pub errors: u64,
    /// Iterations per second.
    pub rps: f64,
    /// The median iteration latency, in ms.
    pub p50_ms: f64,
    /// The 95th percentile latency, in ms.
    pub p95_ms: f64,
    /// The 99th percentile latency, in ms.
    pub p99_ms: f64,
    /// The slowest iteration, in ms.
    pub max_ms: f64,
    /// The thresholds the whole run breached.
    pub breaches: Vec<LoadBreach>,
    /// Failed iterations by api and status, most first.
    pub error_kinds: Vec<ErrorKind>,
    /// The shared action's failure that kept the users from starting.
    pub blocked: Option<String>,
    /// The history's number for the run, when it was recorded.
    pub run_id: Option<u64>,
    /// The seed of the run's random values.
    pub seed: u64,
    /// Problems that did not change the result, such as a history that could not be written.
    pub notices: Vec<String>,
}

impl LoadReport {
    /// The history status of a run that ended before the users finished.
    pub(crate) const fn status(&self) -> RunStatus {
        match self.ending {
            LoadEnding::Cancelled => RunStatus::Cancelled,
            _ => RunStatus::Failed,
        }
    }

    /// The summary `vc load` prints once the run is over.
    #[must_use]
    pub fn text(&self) -> String {
        let (scenario, ending, seconds) = (&self.scenario, self.ending.as_str(), self.seconds);
        let (iterations, rps, errors) = (self.iterations, self.rps, self.errors);
        let (p50, p95, p99, max) = (self.p50_ms, self.p95_ms, self.p99_ms, self.max_ms);
        let mut lines = vec![
            format!("{scenario} {ending} after {seconds} s · {iterations} iterations · {rps:.1} rps · {errors} errors"),
            format!("latency p50 {p50:.1} ms · p95 {p95:.1} ms · p99 {p99:.1} ms · max {max:.1} ms"),
        ];
        lines.extend(self.breaches.iter().map(|breach| {
            let owner = breach.test.as_ref().map_or_else(String::new, |test| format!("{test}: "));
            format!("breached {owner}{} at {:.3}", breach.metric, breach.actual)
        }));
        lines.extend(self.error_kinds.iter().map(|kind| {
            let api = kind.api.as_deref().unwrap_or("(inline request)");
            let status = kind.status.map_or_else(|| "no response".to_owned(), |status| status.to_string());
            format!("errors {api} {status} × {}", kind.count)
        }));
        lines.extend(self.blocked.iter().map(|blocked| format!("blocked: {blocked}")));
        lines.iter().flat_map(|line| [line.as_str(), "\n"]).collect()
    }

    /// The report as `vc load --json` prints it.
    #[must_use]
    pub fn json(&self) -> Value {
        let breach =
            |breach: &LoadBreach| json!({"metric": breach.metric, "test": breach.test, "actual": breach.actual});
        let kind = |kind: &ErrorKind| json!({"api": kind.api, "status": kind.status, "count": kind.count});
        json!({
            "scenario": self.scenario,
            "ending": self.ending.as_str(),
            "exit": self.exit,
            "seconds": self.seconds,
            "iterations": self.iterations,
            "errors": self.errors,
            "rps": self.rps,
            "p50Ms": self.p50_ms,
            "p95Ms": self.p95_ms,
            "p99Ms": self.p99_ms,
            "maxMs": self.max_ms,
            "breaches": self.breaches.iter().map(breach).collect::<Vec<_>>(),
            "errorKinds": self.error_kinds.iter().map(kind).collect::<Vec<_>>(),
            "blocked": self.blocked,
            "runId": self.run_id,
            "seed": self.seed,
            "notices": self.notices,
        })
    }
}

/// Fills `report` with `outcome`'s whole-run numbers and gives the run's history status.
pub fn fill(report: &mut LoadReport, program: &Program, outcome: &LoadOutcome) -> RunStatus {
    let all = outcome.totals.all();
    let latencies = all.latencies();
    report.seconds = outcome.totals.seconds();
    (report.iterations, report.errors) = (all.count(), all.errors());
    report.rps = rate(all.count(), report.seconds);
    report.p50_ms = latencies.quantile_ms(0.5);
    report.p95_ms = latencies.quantile_ms(0.95);
    report.p99_ms = latencies.quantile_ms(0.99);
    report.max_ms = latencies.max_ms();
    report.breaches = outcome.breaches.iter().map(|breach| self::breach(program, breach)).collect();
    report.ending = match outcome.ending {
        Ending::Finished => LoadEnding::Finished,
        Ending::Aborted => LoadEnding::Aborted,
        _ => LoadEnding::Cancelled,
    };
    let (code, status) = exit(outcome.ending, !outcome.breaches.is_empty());
    report.exit = code;
    status
}

fn breach(program: &Program, breach: &Breach) -> LoadBreach {
    let test = breach.test.and_then(|test| program.str(test.sym())).map(str::to_owned);
    LoadBreach { metric: breach.metric.clone(), test, actual: breach.actual }
}

/// `count` per second over `seconds`.
#[expect(clippy::cast_precision_loss, reason = "a rate needs no more than f64's 52 bits of precision")]
pub fn rate(count: u64, seconds: u64) -> f64 {
    if seconds == 0 { 0.0 } else { count as f64 / seconds as f64 }
}
