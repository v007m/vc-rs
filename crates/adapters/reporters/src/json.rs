use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::{ActionId, Event, InstanceId, Names, OwnerKey};

use crate::collect::Collector;
use crate::{Destination, ReporterContext, encode, text};

/// The `json` console format: one JSON object per event and line, for tools (NDJSON). Tests
/// carry their label, duration and failure; the last line is the run's summary.
#[derive(Debug)]
pub struct JsonReporter {
    names: Names,
    destination: Arc<dyn Destination>,
    collector: Collector,
}

impl JsonReporter {
    /// A reporter printing to the context's destination.
    pub fn new(context: &ReporterContext) -> Self {
        let names = Names::new(context.program(), context.plan());
        Self { names, destination: context.destination(), collector: Collector::new(context.clock()) }
    }

    fn line(&self, value: &Value) -> Result<(), ReportError> {
        self.destination.print(&format!("{value}\n"))
    }

    fn encode(&self, event: &Event) -> Value {
        let names = &self.names;
        let (kind, fields) = match event {
            Event::RunStarted { files, instances } => ("runStarted", json!({"files": files, "instances": instances})),
            Event::FileStarted { file } => ("fileStarted", json!({"file": names.file(*file)})),
            Event::FileFinished { file, totals } => {
                ("fileFinished", json!({"file": names.file(*file), "totals": encode::totals(*totals)}))
            }
            Event::ActionStarted { action, owner } => ("actionStarted", self.action(*action, *owner)),
            Event::ActionFinished { action, owner, outcome } => {
                let (status, failure) = encode::outcome(names, outcome);
                let fields = with(self.action(*action, *owner), json!({"status": status, "failure": failure}));
                ("actionFinished", fields)
            }
            Event::Exchanged { action, owner, exchange } => {
                ("exchanged", with(self.action(*action, *owner), encode::exchange(exchange)))
            }
            Event::CheckEvaluated { action, owner, check } => {
                let fields = with(self.action(*action, *owner), json!({"check": encode::check(names, check)}));
                ("checkEvaluated", fields)
            }
            Event::Warned { action, owner, warning } => {
                ("warned", with(self.action(*action, *owner), json!({"warning": encode::failure(names, warning)})))
            }
            Event::CleanupFinished { cleanup } => {
                ("cleanupFinished", json!({"cleanup": encode::cleanup(names, cleanup)}))
            }
            Event::CleanupsAbandoned { owner, cleanups } => {
                let cleanups: Vec<String> = cleanups.iter().map(|action| names.action(*action)).collect();
                ("cleanupsAbandoned", json!({"owner": encode::owner(names, *owner), "cleanups": cleanups}))
            }
            Event::TestFinished { .. } => ("testFinished", self.test()),
            Event::RunFinished { status, totals } => {
                ("runFinished", json!({"status": encode::run_status(*status), "totals": encode::totals(*totals)}))
            }
            tick @ Event::LoadTick { .. } => ("loadTick", encode::tick(tick)),
        };
        with(json!({"event": kind}), fields)
    }

    fn action(&self, action: ActionId, owner: OwnerKey) -> Value {
        json!({"action": self.names.action(action), "owner": encode::owner(&self.names, owner)})
    }

    /// The test the collector just finished.
    fn test(&self) -> Value {
        let Some(record) = self.collector.tests.last() else { return json!({}) };
        let result = &record.result;
        json!({
            "test": self.names.test(result.test),
            "instance": result.instance.map(InstanceId::index),
            "label": self.names.instance(result),
            "status": text::status(result.status),
            "attempts": result.attempts,
            "durationMs": text::millis(record.duration()),
            "failure": result.failure.as_ref().map(|failure| encode::failure(&self.names, failure)),
            "skipReason": result.skip_reason,
        })
    }
}

#[async_trait]
impl Reporter for JsonReporter {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError> {
        self.collector.take(event);
        self.line(&self.encode(event))
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        let artifacts: Vec<Value> =
            run.artifacts.iter().map(|a| json!({"reporter": a.reporter, "path": a.path})).collect();
        self.line(&json!({
            "event": "summary",
            "status": encode::run_status(run.status),
            "totals": encode::totals(run.totals),
            "startedMs": run.started_ms,
            "durationMs": text::millis(run.duration),
            "profiles": run.profiles,
            "artifacts": artifacts,
        }))?;
        Ok(Vec::new())
    }
}

/// `base` with the keys of `more` added.
fn with(mut base: Value, more: Value) -> Value {
    if let (Value::Object(base), Value::Object(more)) = (&mut base, more) {
        base.extend(more);
    }
    base
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
