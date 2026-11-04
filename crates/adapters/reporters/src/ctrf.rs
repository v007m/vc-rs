//! The canonical result (D43): a CTRF document, with what CTRF has no field for (lifecycle,
//! blocked-by, checks with their source lines) under its `extra` objects.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::{Event, InstanceId, TestStatus};

use crate::collect::{Collector, TestRecord};
use crate::names::Names;
use crate::{Destination, ReporterContext, encode, text};

/// The file the reporter writes.
const FILE: &str = "ctrf.json";

/// The CTRF specification version the document follows.
const SPEC_VERSION: &str = "0.2.0";

/// Writes `ctrf.json` when the run finishes.
#[derive(Debug)]
pub struct CtrfReporter {
    names: Names,
    destination: Arc<dyn Destination>,
    collector: Collector,
}

impl CtrfReporter {
    /// A reporter saving to the context's destination.
    pub fn new(context: &ReporterContext) -> Self {
        let names = Names::new(context.program(), context.plan());
        Self { names, destination: context.destination(), collector: Collector::new(context.clock()) }
    }

    fn document(&self, run: &RunSummary) -> Value {
        let tests: Vec<Value> = self.collector.tests.iter().map(|record| self.test(record, run.started_ms)).collect();
        json!({
            "reportFormat": "CTRF",
            "specVersion": SPEC_VERSION,
            "results": {
                "tool": {"name": "vibranium", "version": env!("CARGO_PKG_VERSION")},
                "summary": summary(run),
                "tests": tests,
                "extra": self.extra(run),
            },
        })
    }

    /// The run's own record: what CTRF's summary cannot say.
    fn extra(&self, run: &RunSummary) -> Value {
        let names = &self.names;
        let lifecycle: Vec<Value> = self.collector.lifecycle.iter().map(|step| encode::step(names, step)).collect();
        let cleanups: Vec<Value> = self.collector.cleanups.iter().map(|c| encode::cleanup(names, c)).collect();
        let abandoned = self.collector.abandoned.iter();
        let abandoned: Vec<Value> = abandoned
            .map(|(owner, action)| json!({"action": names.action(*action), "owner": encode::owner(names, *owner)}))
            .collect();
        json!({
            "status": encode::run_status(run.status),
            "totals": encode::totals(run.totals),
            "profiles": run.profiles,
            "lifecycle": lifecycle,
            "cleanups": cleanups,
            "abandoned": abandoned,
        })
    }

    fn test(&self, record: &TestRecord, started_ms: u64) -> Value {
        let result = &record.result;
        let location = self.names.test_location(result.test);
        let start = record.start.map_or(0, |start| started_ms.saturating_add(text::millis(start)));
        let stop = record.start.map_or(0, |_| started_ms.saturating_add(text::millis(record.stop)));
        let mut test = json!({
            "name": self.names.instance(result),
            "status": status(result.status),
            "duration": text::millis(record.duration()),
            "start": start,
            "stop": stop,
            "rawStatus": text::status(result.status),
            "tags": self.names.tags(result.test),
            "filePath": location.as_ref().map(|at| at.path.clone()),
            "line": location.map(|at| at.line),
            "retries": result.attempts.saturating_sub(1),
            "flaky": result.status == TestStatus::Flaky,
            "extra": self.test_extra(record),
        });
        if let Some(message) = message(record) {
            test["message"] = Value::String(message);
        }
        test
    }

    fn test_extra(&self, record: &TestRecord) -> Value {
        let (names, result) = (&self.names, &record.result);
        let slot = result.instance.map(InstanceId::index);
        let blocked_by = result.failure.as_ref().filter(|_| result.status == TestStatus::Blocked);
        json!({
            "id": names.test(result.test),
            "file": names.file_of(result.test),
            "instance": slot,
            "attempts": result.attempts,
            "failure": result.failure.as_ref().map(|failure| encode::failure(names, failure)),
            "blockedBy": blocked_by.and_then(|failure| failure.action).map(|action| names.action(action)),
            "skipReason": result.skip_reason,
            "checks": record.steps.iter().flat_map(|s| &s.checks).map(|c| encode::check(names, c)).collect::<Vec<_>>(),
            "steps": record.steps.iter().map(|step| encode::step(names, step)).collect::<Vec<_>>(),
        })
    }
}

#[async_trait]
impl Reporter for CtrfReporter {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError> {
        self.collector.take(event);
        Ok(())
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        let bytes = serde_json::to_vec_pretty(&self.document(run))
            .map_err(|error| ReportError::Encode { reporter: "ctrf".to_owned(), reason: error.to_string() })?;
        let path = self.destination.save(FILE, &bytes)?;
        Ok(vec![Artifact::new("ctrf", path)])
    }
}

/// CTRF's summary: blocked counts as failed and flaky as passed, as the exit code does.
fn summary(run: &RunSummary) -> Value {
    let totals = run.totals;
    json!({
        "tests": totals.total(),
        "passed": totals.passed + totals.flaky,
        "failed": totals.failures(),
        "pending": 0,
        "skipped": totals.skipped,
        "other": 0,
        "start": run.started_ms,
        "stop": run.started_ms.saturating_add(text::millis(run.duration)),
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

/// The failure that decided the status, or the skip reason.
fn message(record: &TestRecord) -> Option<String> {
    let result = &record.result;
    match result.status {
        TestStatus::Skipped => result.skip_reason.clone(),
        TestStatus::Passed => None,
        TestStatus::Failed | TestStatus::Blocked | TestStatus::Flaky => {
            result.failure.as_ref().map(|failure| failure.message.clone())
        }
    }
}

#[cfg(test)]
#[path = "ctrf_tests.rs"]
mod tests;
