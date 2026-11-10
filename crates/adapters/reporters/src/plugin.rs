//! A reporter plugin (G6): `reports.formats: [<pluginId>]` sends each event to the plugin's
//! `reporter/onEvent`, then its `reporter/finish` returns the files it wants written. Events go
//! through a bounded queue drained by a task of their own, so a slow plugin never holds up the run.

use std::path::{Component, Path};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use vc_interfaces::{Artifact, CALL_DEADLINE, PluginError, PluginHost, ReportError, Reporter, RunSummary};
use vc_model::Event;

use crate::json::{JsonReporter, summary};
use crate::{Destination, ReporterContext};

/// How many encoded events wait for the plugin before the run waits for it.
const QUEUE: usize = 1024;

/// A [`Reporter`] that hands the run to a reporter plugin.
///
/// Events are encoded as the `json` console format prints them and queued; a task sends them to
/// `reporter/onEvent` in order. `reporter/finish` is sent once the queue is drained, and answers
/// `{"files": [{"name", "content"}]}`; each file is saved in the run's report folder under a
/// relative `name` that stays inside it.
pub struct PluginReporter {
    host: Arc<dyn PluginHost>,
    encoder: JsonReporter,
    destination: Arc<dyn Destination>,
    queue: Queue,
}

/// Where the reporter's events go.
enum Queue {
    /// No event yet; the drain starts with the first.
    Idle,
    /// The queue, and the task sending what it holds to the plugin.
    Draining(mpsc::Sender<Value>, JoinHandle<Result<(), String>>),
    /// The plugin failed an event, for this reason; it gets no more.
    Failed(String),
}

impl PluginReporter {
    /// A reporter calling the plugin `host`, saving its files where `context` writes.
    #[must_use]
    pub fn new(host: Arc<dyn PluginHost>, context: &ReporterContext) -> Self {
        Self { host, encoder: JsonReporter::new(context), destination: context.destination(), queue: Queue::Idle }
    }

    /// Waits until the plugin has every queued event: why it failed one, if it did.
    async fn drained(&mut self) -> Result<(), String> {
        match std::mem::replace(&mut self.queue, Queue::Idle) {
            Queue::Idle => Ok(()),
            Queue::Failed(reason) => Err(reason),
            Queue::Draining(sender, task) => {
                drop(sender);
                task.await.unwrap_or_else(|error| Err(error.to_string()))
            }
        }
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
        if let Queue::Failed(reason) = &self.queue {
            return Err(self.failed(reason.clone()));
        }
        let event = self.encoder.event(event);
        if matches!(self.queue, Queue::Idle) {
            let (sender, events) = mpsc::channel(QUEUE);
            self.queue = Queue::Draining(sender, tokio::spawn(send(Arc::clone(&self.host), events)));
        }
        if let Queue::Draining(sender, _) = &self.queue
            && sender.send(event).await.is_ok()
        {
            return Ok(());
        }
        let reason = self.drained().await.err().unwrap_or_else(|| "it stopped taking events".to_owned());
        self.queue = Queue::Failed(reason.clone());
        Err(self.failed(reason))
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        self.drained().await.map_err(|reason| self.failed(reason))?;
        let called = self.host.call("reporter/finish", json!({"summary": summary(run)}), CALL_DEADLINE).await;
        let answer = called.map_err(|error| self.failed(error.to_string()))?;
        let files = answer.get("files").and_then(Value::as_array).cloned().unwrap_or_default();
        files.iter().map(|file| self.save(file)).collect()
    }
}

/// Sends each queued event to the plugin's `reporter/onEvent`, in order, until the queue closes
/// or a call fails: why it failed.
async fn send(host: Arc<dyn PluginHost>, mut events: mpsc::Receiver<Value>) -> Result<(), String> {
    while let Some(event) = events.recv().await {
        let called = host.call("reporter/onEvent", json!({"event": event}), CALL_DEADLINE).await;
        called.map_err(|error: PluginError| error.to_string())?;
    }
    Ok(())
}

/// Whether `name` is a relative path that stays inside the folder it is joined to.
fn inside(name: &str) -> bool {
    let path = Path::new(name);
    !name.is_empty() && path.components().all(|part| matches!(part, Component::Normal(_)))
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
