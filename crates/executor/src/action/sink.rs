//! Cleanup sinks: a nested action hands over the cleanups it registered as soon as it finishes,
//! so dropping what ran it (past a `timeout`) loses none. Lock-free: an unbounded channel whose
//! sends never wait, so a `Drop` can hand over too.

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::ActionRequest;
use crate::{ActionReport, Frame, PendingCleanup};

/// Where the actions of a frame hand their cleanups; cloned into each nested frame.
#[derive(Clone, Debug)]
pub struct CleanupSink(UnboundedSender<PendingCleanup>);

impl CleanupSink {
    /// Hands `cleanups` to `sink`, oldest first; those it cannot take come back, all of them
    /// when there is no sink.
    #[must_use]
    pub fn keep(sink: Option<&Self>, cleanups: Vec<PendingCleanup>) -> Vec<PendingCleanup> {
        let Some(Self(sender)) = sink else { return cleanups };
        cleanups.into_iter().filter_map(|cleanup| sender.send(cleanup).err().map(|refused| refused.0)).collect()
    }
}

/// Gathers the cleanups the actions under it hand over. Dropped before they are taken, it hands
/// them on to the sink it was made under.
#[derive(Debug)]
pub struct CleanupCollector {
    sink: CleanupSink,
    inbox: UnboundedReceiver<PendingCleanup>,
    parent: Option<CleanupSink>,
}

impl CleanupCollector {
    /// Runs `run` on `request` with a collector of its own as the frame's sink. The report
    /// carries every cleanup handed over, oldest first, then its own; dropped, the collector
    /// hands them to `request`'s sink.
    pub async fn around<F>(request: &ActionRequest, run: impl FnOnce(ActionRequest) -> F) -> ActionReport
    where
        F: Future<Output = ActionReport>,
    {
        let mut kept = Self::under(request.frame.sink.clone());
        let mut report = run(ActionRequest { frame: kept.frame(&request.frame), ..request.clone() }).await;
        report.cleanups = kept.take(report.cleanups);
        report
    }

    /// A collector that hands what is left on to `parent` when dropped.
    pub fn under(parent: Option<CleanupSink>) -> Self {
        let (sender, inbox) = mpsc::unbounded_channel();
        Self { sink: CleanupSink(sender), inbox, parent }
    }

    /// `frame` with this collector as its sink.
    pub fn frame(&self, frame: &Frame) -> Frame {
        Frame { sink: Some(self.sink.clone()), ..frame.clone() }
    }

    /// Takes `cleanups` in after the ones handed over so far.
    pub fn hand(&self, cleanups: Vec<PendingCleanup>) {
        drop(CleanupSink::keep(Some(&self.sink), cleanups));
    }

    /// The cleanups handed over so far, oldest first, then `rest`.
    pub fn take(&mut self, rest: Vec<PendingCleanup>) -> Vec<PendingCleanup> {
        let mut taken: Vec<_> = std::iter::from_fn(|| self.inbox.try_recv().ok()).collect();
        taken.extend(rest);
        taken
    }
}

impl Drop for CleanupCollector {
    fn drop(&mut self) {
        let left = self.take(Vec::new());
        drop(CleanupSink::keep(self.parent.as_ref(), left));
    }
}

#[cfg(test)]
#[path = "sink_tests.rs"]
mod tests;
