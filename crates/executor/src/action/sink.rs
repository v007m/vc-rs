//! Cleanup sinks: a nested action hands over the cleanups it registered as soon as it finishes,
//! so dropping what ran it (past a `timeout`) loses none. Lock-free: an unbounded channel whose
//! sends never wait, so a `Drop` can hand over too.

use std::time::Duration;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::ActionRequest;
use crate::{ActionReport, Frame, PendingCleanup};

/// How long [`CleanupCollector::settle`] waits for the frames under it to let go of its sink.
const HANDOVER: Duration = Duration::from_secs(1);

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
    sink: Option<CleanupSink>,
    inbox: UnboundedReceiver<PendingCleanup>,
    parent: Option<CleanupSink>,
}

impl CleanupCollector {
    /// Runs `run` on `request` with a collector of its own as the frame's sink. The report
    /// carries every cleanup handed over, oldest first, then its own; dropped, the collector
    /// hands them to `request`'s sink.
    pub async fn around<F>(request: ActionRequest, run: impl FnOnce(ActionRequest) -> F) -> ActionReport
    where
        F: Future<Output = ActionReport>,
    {
        let (mut report, kept) = Self::gather(request, run).await;
        report.cleanups = kept.settle(std::mem::take(&mut report.cleanups)).await;
        report
    }

    /// What `run` on `request` gives, with a collector of its own as the frame's sink, and that
    /// collector, holding the cleanups handed over to it; dropped, it hands them to `request`'s
    /// sink.
    pub async fn gather<F: Future>(request: ActionRequest, run: impl FnOnce(ActionRequest) -> F) -> (F::Output, Self) {
        let kept = Self::under(request.frame.sink.clone());
        let output = run(ActionRequest { frame: kept.frame(&request.frame), ..request }).await;
        (output, kept)
    }

    /// A collector that hands what is left on to `parent` when dropped.
    pub fn under(parent: Option<CleanupSink>) -> Self {
        let (sender, inbox) = mpsc::unbounded_channel();
        Self { sink: Some(CleanupSink(sender)), inbox, parent }
    }

    /// `frame` with this collector as its sink.
    pub fn frame(&self, frame: &Frame) -> Frame {
        Frame { sink: self.sink.clone(), ..frame.clone() }
    }

    /// Takes `cleanups` in after the ones handed over so far.
    pub fn hand(&self, cleanups: Vec<PendingCleanup>) {
        drop(CleanupSink::keep(self.sink.as_ref(), cleanups));
    }

    /// The cleanups handed over so far, oldest first, then `rest`.
    pub fn take(&mut self, rest: Vec<PendingCleanup>) -> Vec<PendingCleanup> {
        let mut taken: Vec<_> = std::iter::from_fn(|| self.inbox.try_recv().ok()).collect();
        taken.extend(rest);
        taken
    }

    /// Like [`Self::take`], once every frame holding this collector's sink has let go of it, or
    /// after [`HANDOVER`]: a task aborted under it (a loop's turn) drops its own collectors, and
    /// so hands over, only when the runtime next polls it.
    pub async fn settle(mut self, rest: Vec<PendingCleanup>) -> Vec<PendingCleanup> {
        self.sink = None;
        let mut taken = Vec::new();
        let all = async {
            while let Some(cleanup) = self.inbox.recv().await {
                taken.push(cleanup);
            }
        };
        let _unsettled = tokio::time::timeout(HANDOVER, all).await;
        taken.extend(self.take(rest));
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
