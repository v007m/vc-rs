//! The cancellation tree (run → file → instance) and the grace period.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

/// How a guarded piece of work ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ran<T> {
    /// It finished with this result.
    Done(T),
    /// Cancellation cut it off, or it never started.
    Cut,
    /// The run was aborted.
    Aborted,
}

/// One frame's cancellation.
///
/// `work` stops new work and starts the grace period for work in flight; `hurry` ends the grace
/// period (a second Ctrl-C); `abort` stops everything, cleanups included (a third). A child frame gets a child `work` token, so cancelling a frame cancels its
/// subtree; `hurry` and `abort` are run-wide.
#[derive(Clone, Debug)]
pub struct CancelTree {
    work: CancellationToken,
    hurry: CancellationToken,
    abort: CancellationToken,
    grace: Duration,
}

impl CancelTree {
    /// The run's root, with `grace` for work in flight when cancelled.
    #[must_use]
    pub fn new(grace: Duration) -> Self {
        Self { work: CancellationToken::new(), hurry: CancellationToken::new(), abort: CancellationToken::new(), grace }
    }

    /// A child frame.
    #[must_use]
    pub fn child(&self) -> Self {
        Self { work: self.work.child_token(), ..self.clone() }
    }

    /// Cancels this frame and its children.
    pub fn cancel(&self) {
        self.work.cancel();
    }

    /// Ends the grace period now.
    pub fn skip_grace(&self) {
        self.hurry.cancel();
    }

    /// Stops everything now.
    pub fn abort(&self) {
        self.abort.cancel();
    }

    /// Whether this frame is cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.work.is_cancelled()
    }

    /// Resolves when this frame is cancelled.
    pub async fn cancelled(&self) {
        self.work.cancelled().await;
    }

    /// Resolves when the run is aborted.
    pub async fn aborted(&self) {
        self.abort.cancelled().await;
    }

    /// Runs `work` unless the frame is already cancelled. Once cancelled, work in flight gets the
    /// grace period to finish.
    pub async fn guard<T>(&self, work: impl Future<Output = T>) -> Ran<T> {
        if let Some(stopped) = self.stopped() {
            return stopped;
        }
        let mut work = std::pin::pin!(work);
        tokio::select! {
            biased;
            () = self.abort.cancelled() => return Ran::Aborted,
            out = &mut work => return Ran::Done(out),
            () = self.work.cancelled() => {}
        }
        self.within_grace(work).await
    }

    /// How work would end that has not started yet, if the frame is stopped.
    fn stopped<T>(&self) -> Option<Ran<T>> {
        if self.abort.is_cancelled() {
            Some(Ran::Aborted)
        } else if self.work.is_cancelled() {
            Some(Ran::Cut)
        } else {
            None
        }
    }

    /// Lets cancelled work finish within the grace period, unless it is skipped or the run aborts.
    async fn within_grace<T>(&self, work: Pin<&mut impl Future<Output = T>>) -> Ran<T> {
        tokio::select! {
            biased;
            () = self.abort.cancelled() => Ran::Aborted,
            out = work => Ran::Done(out),
            () = self.hurry.cancelled() => Ran::Cut,
            () = tokio::time::sleep(self.grace) => Ran::Cut,
        }
    }

    /// Runs a cleanup: cancellation does not stop it, only an abort does.
    pub async fn cleanup<T>(&self, work: impl Future<Output = T>) -> Ran<T> {
        tokio::select! {
            biased;
            () = self.abort.cancelled() => Ran::Aborted,
            out = work => Ran::Done(out),
        }
    }
}

#[cfg(test)]
#[path = "cancel_tests.rs"]
mod tests;
