//! The canonical result (D43): a CTRF document, with what CTRF has no field for (lifecycle,
//! blocked-by, checks with their source lines) under its `extra` objects.

use serde_json::{Value, json};
use vc_interfaces::{Artifact, ReportError};
use vc_model::TestStatus;

use crate::view::{RunFacts, TestView};
use crate::{Destination, Format, RunSnapshot, encode, text};

/// The file the format writes.
const FILE: &str = "ctrf.json";

/// The CTRF specification version the document follows.
const SPEC_VERSION: &str = "0.2.0";

/// Writes `ctrf.json`.
#[derive(Clone, Copy, Debug)]
pub struct CtrfFormat;

impl Format for CtrfFormat {
    fn render(&self, snapshot: &RunSnapshot, destination: &dyn Destination) -> Result<Vec<Artifact>, ReportError> {
        let bytes = serde_json::to_vec_pretty(&document(snapshot))
            .map_err(|error| ReportError::Encode { reporter: "ctrf".to_owned(), reason: error.to_string() })?;
        let path = destination.save(FILE, &bytes)?;
        Ok(vec![Artifact::new("ctrf", path)])
    }
}

fn document(snapshot: &RunSnapshot) -> Value {
    let run = &snapshot.run;
    let tests: Vec<Value> = snapshot.tests.iter().map(|view| test(view, run.started_ms)).collect();
    json!({
        "reportFormat": "CTRF",
        "specVersion": SPEC_VERSION,
        "results": {
            "tool": {"name": "vibranium", "version": env!("CARGO_PKG_VERSION")},
            "summary": summary(run),
            "tests": tests,
            "extra": extra(snapshot),
        },
    })
}

/// The run's own record: what CTRF's summary cannot say.
fn extra(snapshot: &RunSnapshot) -> Value {
    let run = &snapshot.run;
    json!({
        "status": encode::run_status(run.status),
        "totals": encode::totals(run.totals),
        "profiles": run.profiles,
        "lifecycle": snapshot.lifecycle,
        "cleanups": snapshot.cleanups,
        "abandoned": snapshot.abandoned,
    })
}

fn test(view: &TestView, started_ms: u64) -> Value {
    let start = view.start_ms.map_or(0, |start| started_ms.saturating_add(start));
    let stop = view.start_ms.map_or(0, |_| started_ms.saturating_add(view.stop_ms));
    let mut test = json!({
        "name": view.label,
        "status": status(view.status),
        "duration": view.duration_ms(),
        "start": start,
        "stop": stop,
        "rawStatus": text::status(view.status),
        "tags": view.tags,
        "filePath": view.location.as_ref().map(|at| at.path.clone()),
        "line": view.location.as_ref().map(|at| at.line),
        "retries": view.attempts.saturating_sub(1),
        "flaky": view.status == TestStatus::Flaky,
        "extra": test_extra(view),
    });
    if let Some(message) = &view.message {
        test["message"] = Value::String(message.clone());
    }
    test
}

fn test_extra(view: &TestView) -> Value {
    json!({
        "id": view.id,
        "file": view.file,
        "instance": view.instance,
        "attempts": view.attempts,
        "failure": view.failure,
        "blockedBy": view.blocked_by,
        "skipReason": view.skip_reason,
        "checks": view.checks().collect::<Vec<_>>(),
        "steps": view.steps,
    })
}

/// CTRF's summary: blocked counts as failed and flaky as passed, as the exit code does.
fn summary(run: &RunFacts) -> Value {
    let totals = run.totals;
    json!({
        "tests": totals.total(),
        "passed": totals.passed + totals.flaky,
        "failed": totals.failures(),
        "pending": 0,
        "skipped": totals.skipped,
        "other": 0,
        "start": run.started_ms,
        "stop": run.started_ms.saturating_add(run.duration_ms),
    })
}

/// CTRF's status; `rawStatus` keeps Vibranium's own.
const fn status(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Passed | TestStatus::Flaky => "passed",
        TestStatus::Failed | TestStatus::Blocked => "failed",
        TestStatus::Skipped => "skipped",
    }
}

#[cfg(test)]
#[path = "ctrf_tests.rs"]
mod tests;
