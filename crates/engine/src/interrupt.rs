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
}

/// The receiving end, handed to [`crate::Engine::run`].
#[derive(Debug)]
pub struct Interrupts {
    pub(crate) inbox: UnboundedReceiver<()>,
}

/// A connected [`Interrupter`] and [`Interrupts`].
#[must_use]
pub fn interrupts() -> (Interrupter, Interrupts) {
    let (inbox, receiver) = mpsc::unbounded_channel();
    (Interrupter { inbox }, Interrupts { inbox: receiver })
}
