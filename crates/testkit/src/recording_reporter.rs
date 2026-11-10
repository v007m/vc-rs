use std::sync::mpsc::{self, Receiver, Sender};

use async_trait::async_trait;
use vc_interfaces::{Artifact, ReportError, Reporter, RunSummary};
use vc_model::Event;

/// What a [`RecordingReporter`] was given, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reported {
    /// An event.
    Event(Event),
    /// The summary `finish` was called with.
    Finished(RunSummary),
}

/// A reporter that records what it is given and writes nothing, for engine tests.
///
/// Records go down a channel to the [`ReportLog`] handed out by [`RecordingReporter::new`],
/// so a test can read them while the reporter is owned by its task.
#[derive(Debug)]
pub struct RecordingReporter {
    records: Sender<Reported>,
}

/// The receiving end of a [`RecordingReporter`].
#[derive(Debug)]
pub struct ReportLog(Receiver<Reported>);

impl ReportLog {
    /// Everything recorded since the last call, oldest first.
    #[must_use]
    pub fn take(&self) -> Vec<Reported> {
        self.0.try_iter().collect()
    }
}

impl RecordingReporter {
    /// A reporter and the log it records into.
    #[must_use]
    pub fn new() -> (Self, ReportLog) {
        let (records, inbox) = mpsc::channel();
        (Self { records }, ReportLog(inbox))
    }
}

#[async_trait]
impl Reporter for RecordingReporter {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError> {
        self.records.send(Reported::Event(event.clone())).ok();
        Ok(())
    }

    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError> {
        self.records.send(Reported::Finished(run.clone())).ok();
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{Reporter, RunSummary};
    use vc_model::{Event, RunStatus, Totals};

    use super::{RecordingReporter, Reported};
    use crate::reporter_suite;

    #[tokio::test]
    async fn passes_the_reporter_suite() {
        reporter_suite(&mut || Box::new(RecordingReporter::new().0)).await;
    }

    #[tokio::test]
    async fn records_events_then_the_summary() {
        let (mut reporter, log) = RecordingReporter::new();
        let started = Event::RunStarted { files: 1, instances: 2 };
        let summary = RunSummary::new(RunStatus::Passed, Totals::default());
        reporter.on_event(&started).await.unwrap();
        assert_eq!(reporter.finish(&summary).await.unwrap(), []);
        assert_eq!(log.take(), [Reported::Event(started), Reported::Finished(summary)]);
        assert!(log.take().is_empty(), "take drains");
    }
}
