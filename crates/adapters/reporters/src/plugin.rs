//! A reporter plugin (G6): `reports.formats: [<pluginId>]` sends each event to the plugin's
//! `reporter/onEvent`, then its `reporter/finish` returns the files it wants written.

use std::path::{Component, Path};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::{Artifact, CALL_DEADLINE, PluginError, PluginHost, ReportError, Reporter, RunSummary};
use vc_model::Event;

use crate::json::{JsonReporter, summary};
use crate::{Destination, ReporterContext};

/// A [`Reporter`] that hands the run to a reporter plugin.
///
/// Events are encoded as the `json` console format prints them. `reporter/finish` answers `{"files": [{"name", "content"}]}`; each file
/// is saved in the run's report folder under a relative `name` that stays inside it.
pub struct PluginReporter {
    host: Arc<dyn PluginHost>,
    encoder: JsonReporter,
    destination: Arc<dyn Destination>,
}

impl PluginReporter {
    /// A reporter calling the plugin `host`, saving its files where `context` writes.
    #[must_use]
    pub fn new(host: Arc<dyn PluginHost>, context: &ReporterContext) -> Self {
        Self { host, encoder: JsonReporter::new(context), destination: context.destination() }
    }

    fn id(&self) -> String {
        self.host.manifest().id.clone()
    }

    fn failed(&self, reason: impl Into<String>) -> ReportError {
        ReportError::Write { location: format!("the `{}` report", self.id()), reason: reason.into() }
    }

    /// Saves one file the plugin returned.
    fn save(&self, file: &Value) -> Result<Artifact, ReportError> {
        let name = file.get("name").and_then(Value::as_str).unwrap_or_default();
        if !inside(name) {
            return Err(self.failed(format!("`{name}` is not a file name inside the report folder")));
        }
        let content = file.get("content").and_then(Value::as_str).unwrap_or_default();
        Ok(Artifact::new(self.id(), self.destination.save(name, content.as_bytes())?))
    }
}

impl std::fmt::Debug for PluginReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginReporter").field("plugin", &self.id()).finish_non_exhaustive()
    }
}

#[async_trait]
impl Reporter for PluginReporter {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError> {
        let event = self.encoder.event(event);
        let called = self.host.call("reporter/onEvent", json!({"event": event}), CALL_DEADLINE).await;
        called.map(drop).map_err(|error: PluginError| self.failed(error.to_string()))
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        let called = self.host.call("reporter/finish", json!({"summary": summary(run)}), CALL_DEADLINE).await;
        let answer = called.map_err(|error| self.failed(error.to_string()))?;
        let files = answer.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
        files.iter().map(|file| self.save(file)).collect()
    }
}

/// Whether `name` is a relative path that stays inside the folder it is joined to.
fn inside(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty() && path.components().all(|part| matches!(part, Component::Normal(_)))
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
