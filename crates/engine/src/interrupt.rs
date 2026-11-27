//! How an app interrupts a run, such as on Ctrl-C.

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// Sends interrupts to a run: the first cancels it (work in flight gets a grace period and
/// cleanups run), the second ends the grace period, the third abandons the cleanups.
#[derive(Clone, Debug)]
pub struct Interrupter {
    inbox: UnboundedSender<()>,
}

impl Interrupter {
    /// One interrupt; nothing once the run has finished.
    pub fn interrupt(&self) {
        self.inbox.send(()).ok();
    }

    /// Three interrupts at once, as the web UI's abandon (D552): the run is cancelled, its grace
    /// period ended and its cleanups abandoned; nothing once the run has finished.
    pub fn abandon(&self) {
        for _ in 0..ABANDON {
            self.interrupt();
        }
    }

    /// Whether the run these interrupts were for has finished, its [`Interrupts`] gone.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.inbox.is_closed()
    }
}

/// The receiving end, handed to [`crate::Engine::run`].
#[derive(Debug)]
pub struct Interrupts {
    inbox: UnboundedReceiver<()>,
    seen: u8,
}

/// The interrupt that abandons the cleanups.
const ABANDON: u8 = 3;

/// A connected [`Interrupter`] and [`Interrupts`].
#[must_use]
pub fn interrupts() -> (Interrupter, Interrupts) {
    let (inbox, receiver) = mpsc::unbounded_channel();
    (Interrupter { inbox }, Interrupts { inbox: receiver, seen: 0 })
}

impl Interrupts {
    /// The next interrupt, counted; `None` once every [`Interrupter`] is gone.
    pub(crate) async fn next(&mut self) -> Option<()> {
        let next = self.inbox.recv().await;
        if next.is_some() {
            self.seen = self.seen.saturating_add(1);
        }
        next
    }

    /// How many interrupts have come so far.
    pub(crate) const fn seen(&self) -> u8 {
        self.seen
    }

    /// Ready once the third interrupt has come, at once if it came during the run, so the
    /// cleanups still to run are abandoned; never, if every [`Interrupter`] goes first.
    pub(crate) async fn abandoned(&mut self) {
        while self.seen < ABANDON {
            if self.next().await.is_none() {
                std::future::pending::<()>().await;
            }
        }
    }
}

#[cfg(test)]
#[path = "interrupt_tests.rs"]
mod tests;
