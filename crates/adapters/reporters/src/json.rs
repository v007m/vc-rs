use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value, json};
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::{ActionId, Event, InstanceId, Names, OwnerKey};

use crate::collect::Collector;
use crate::teardown::Teardown;
use crate::{Destination, ReporterContext, encode, text};

/// The `json` console format: one JSON object per event and line, for tools (NDJSON). Tests
/// carry their label, duration and failure; the last line is the run's summary, with the
/// teardown blocks that failed (D304).
#[derive(Debug)]
pub struct JsonReporter {
    names: Names,
    teardown: Teardown,
    destination: Arc<dyn Destination>,
    collector: Collector,
}

impl JsonReporter {
    /// A reporter printing to the context's destination.
    pub fn new(context: &ReporterContext) -> Self {
        let names = Names::new(context.program(), context.plan());
        let teardown = Teardown::new(context.plan(), &names);
        Self { names, teardown, destination: context.destination(), collector: Collector::new(context.clock()) }
    }

    fn line(&self, value: &Value) -> Result<(), ReportError> {
        self.destination.print(&format!("{value}\n"))
    }

    /// The `summary` line for `run`, with this reporter's names.
    #[must_use]
    pub fn summary(&self, run: &RunSummary) -> Value {
        let failures = self.teardown.failures(&self.collector.lifecycle);
        with(summary(&self.names, run), json!({"teardownFailures": failures}))
    }

    /// `event` as its line, after the collector has seen it (a `testFinished` line reads the
    /// test it collected).
    pub fn event(&mut self, event: &Event) -> Value {
        self.collector.take(event);
        self.encode(event)
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
            "name": self.names.test_name(result.test),
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
        let line = self.event(event);
        self.line(&line)
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        self.line(&self.summary(run))?;
        Ok(Vec::new())
    }
}

/// The `summary` line: how the run ended, its totals, its profiles with the `name` of each that
/// gives one, and the files its reporters wrote.
pub fn summary(names: &Names, run: &RunSummary) -> Value {
    let artifacts: Vec<Value> = run.artifacts.iter().map(|a| json!({"reporter": a.reporter, "path": a.path})).collect();
    json!({
        "event": "summary",
        "status": encode::run_status(run.status),
        "totals": encode::totals(run.totals),
        "startedMs": run.started_ms,
        "durationMs": text::millis(run.duration),
        "profiles": run.profiles,
        "profileNames": profile_names(names, &run.profiles),
        "artifacts": artifacts,
    })
}

/// The `name` of each of `profiles` that gives one, by its ID.
pub fn profile_names(names: &Names, profiles: &[String]) -> Map<String, Value> {
    let pair = |id: &String| names.profile_name(id).map(|name| (id.clone(), name.into()));
    profiles.iter().filter_map(pair).collect()
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
