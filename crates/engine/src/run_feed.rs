//! The live feed of a run for a watcher (the TUI's Run tab, the server's `Run` stream): the
//! [`Names`] that label it, its events in order, `Missed` when the watcher fell behind, and
//! `Done` once the reporters have finished. Watching never holds up or fails the run.

use std::sync::Arc;

use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::oneshot;
use vc_executor::EventBus;
use vc_model::{Feed, Names, Sequenced};

use crate::RunReport;
use crate::plan::Prepared;

/// Room for this many events between the bus and the relay before it misses some.
const ROOM: usize = 65_536;

/// What a watcher of a run receives.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum RunFeed {
    /// Always first: the names of the run's tests, instances and actions, from its program
    /// and plan, so the watcher can label the events that follow.
    Started(Names),
    /// The next event of the run.
    Event(Sequenced),
    /// The watcher's channel was full and `count` events were dropped; reload them from the
    /// history once the run is done.
    Missed {
        /// How many events were dropped.
        count: u64,
    },
    /// The run ended and its reporters have finished: the history and the report files are
    /// written. Nothing follows it. A run that fails ends the feed without it.
    Done(RunReport),
}

/// Where [`crate::Engine::run`] sends its [`RunFeed`]: a full channel drops events (counted
/// in the next `Missed`) and a closed one is let go; neither changes the run.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RunWatch {
    sender: Sender<RunFeed>,
}

impl RunWatch {
    /// A watch sending into `sender`; its capacity is how far the watcher may fall behind.
    #[must_use]
    pub const fn new(sender: Sender<RunFeed>) -> Self {
        Self { sender }
    }

    /// Relays `bus` to the watcher on a task of its own until the run ends, after the names
    /// of the run `prepared` holds. `Started` waits for room, as `Done` does: it is the
    /// first message, so only a watcher that never reads makes it wait, and the bus counts
    /// what that relay misses meanwhile.
    pub(crate) fn follow(&self, bus: &EventBus, prepared: &Prepared) -> Watching {
        let (done, report) = oneshot::channel();
        let (watcher, feed) = (self.sender.clone(), bus.subscribe(ROOM));
        let names = Names::new(Arc::clone(&prepared.program), &prepared.plan);
        tokio::spawn(async move {
            if watcher.send(RunFeed::Started(names)).await.is_ok() {
                Relay { watcher, missed: 0 }.forward(feed, report).await;
            }
        });
        Watching { done }
    }
}

impl PartialEq for RunWatch {
    fn eq(&self, other: &Self) -> bool {
        self.sender.same_channel(&other.sender)
    }
}

impl Eq for RunWatch {}

/// The run's side of a watch: hands over the report once the reporters have finished.
pub struct Watching {
    done: oneshot::Sender<RunReport>,
}

impl Watching {
    /// Sends `Done(report)` after the events still queued.
    pub(crate) fn finish(self, report: RunReport) {
        self.done.send(report).ok();
    }
}

/// How one offer to the watcher went.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sent {
    Yes,
    Full,
    Closed,
}

/// Passes the bus's feed to the watcher without ever waiting on it, counting what a full
/// channel drops.
struct Relay {
    watcher: Sender<RunFeed>,
    missed: u64,
}

impl Relay {
    async fn forward(mut self, mut feed: Receiver<Feed>, mut report: oneshot::Receiver<RunReport>) {
        loop {
            tokio::select! {
                biased;
                Some(next) = feed.recv() => if self.offer(next) == Sent::Closed { return },
                done = &mut report => {
                    if let Ok(report) = done {
                        self.close(report).await;
                    }
                    return;
                }
            }
        }
    }

    /// Offers `feed`, after any count of missed events.
    fn offer(&mut self, feed: Feed) -> Sent {
        match feed {
            Feed::Missed { count } => {
                self.missed += count;
                self.settle()
            }
            Feed::Event(event) => {
                let sent = match self.settle() {
                    Sent::Yes => self.put(RunFeed::Event(event)),
                    other => other,
                };
                if sent == Sent::Full {
                    self.missed += 1;
                }
                sent
            }
        }
    }

    /// Tells the watcher what it missed, when it missed something and there is room.
    fn settle(&mut self) -> Sent {
        if self.missed == 0 {
            return Sent::Yes;
        }
        let sent = self.put(RunFeed::Missed { count: self.missed });
        if sent == Sent::Yes {
            self.missed = 0;
        }
        sent
    }

    fn put(&self, next: RunFeed) -> Sent {
        match self.watcher.try_send(next) {
            Ok(()) => Sent::Yes,
            Err(TrySendError::Full(_)) => Sent::Full,
            Err(TrySendError::Closed(_)) => Sent::Closed,
        }
    }

    /// The run is over: what was missed, then `Done`, waiting for room since nothing waits on
    /// this task any more.
    async fn close(self, report: RunReport) {
        let missed = (self.missed > 0).then_some(RunFeed::Missed { count: self.missed });
        for next in missed.into_iter().chain([RunFeed::Done(report)]) {
            if self.watcher.send(next).await.is_err() {
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "run_feed_tests.rs"]
mod tests;
