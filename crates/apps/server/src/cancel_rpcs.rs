//! `Cancel` (D551–D553): stops a running `Run` that the client named with its `handle`. `stop`
//! is one Ctrl-C: work in flight gets its grace period and the cleanups run (a second `stop`
//! ends the grace period). `abandon` is the third Ctrl-C at once: the cleanups still to run
//! are abandoned. The running runs are known by their handles, which a task of their own keeps
//! and looks up, so no state is shared; a run that has returned is forgotten at the next ask.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use vc_contracts::engine::v1 as wire;
use vc_engine::Interrupter;

use crate::engine_api::{EngineApi, Rpcs};
use crate::{Code, ConnectError};

/// The longest handle a run takes.
const LONGEST: usize = 64;

/// Serves `Cancel`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("Cancel", cancel)
}

/// The running runs, by handle, which a task of their own keeps.
pub struct Runs {
    asks: mpsc::UnboundedSender<Ask>,
}

/// What the keeper of the runs is asked.
enum Ask {
    /// Keep a new run's interrupter under this handle; whether the handle was free.
    Open(String, Interrupter, oneshot::Sender<bool>),
    /// The run with this handle, while it runs.
    Find(String, oneshot::Sender<Option<Interrupter>>),
}

impl Runs {
    /// The runs, kept by a task on the current Tokio runtime; outside one, none is kept.
    pub fn spawn() -> Self {
        let (asks, inbox) = mpsc::unbounded_channel();
        if let Ok(runtime) = Handle::try_current() {
            drop(runtime.spawn(keep(inbox)));
        }
        Self { asks }
    }

    /// Keeps `interrupter` as the run `handle`'s until the run returns; an empty handle is not
    /// kept, so only closing its stream stops that run. A handle that is not one, or that a
    /// running run already holds, is refused.
    pub(crate) async fn open(&self, handle: &str, interrupter: &Interrupter) -> Result<(), ConnectError> {
        if handle.is_empty() {
            return Ok(());
        }
        named(handle)?;
        let unavailable = || ConnectError::new(Code::Unavailable, "the server keeps no runs to cancel");
        let (reply, free) = oneshot::channel();
        self.asks.send(Ask::Open(handle.to_owned(), interrupter.clone(), reply)).map_err(|_| unavailable())?;
        if free.await.map_err(|_| unavailable())? {
            return Ok(());
        }
        Err(ConnectError::new(Code::AlreadyExists, format!("a run `{handle}` is already running")))
    }

    /// The running run `handle`.
    async fn find(&self, handle: String) -> Option<Interrupter> {
        let (reply, found) = oneshot::channel();
        self.asks.send(Ask::Find(handle, reply)).ok()?;
        found.await.ok().flatten()
    }
}

/// Refuses a handle that is longer than [`LONGEST`] or holds more than ASCII letters, digits,
/// `-` and `_`.
fn named(handle: &str) -> Result<(), ConnectError> {
    let allowed = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
    if handle.len() <= LONGEST && handle.bytes().all(allowed) {
        return Ok(());
    }
    let reason = format!("a run's `handle` is at most {LONGEST} ASCII letters, digits, `-` and `_`");
    Err(ConnectError::new(Code::InvalidArgument, reason))
}

/// Answers `asks` until the API that asks them has gone.
async fn keep(mut asks: mpsc::UnboundedReceiver<Ask>) {
    let mut running: HashMap<String, Interrupter> = HashMap::new();
    while let Some(ask) = asks.recv().await {
        running.retain(|_, run| !run.is_finished());
        match ask {
            Ask::Open(handle, run, reply) => {
                let free = !running.contains_key(&handle);
                if free {
                    drop(running.insert(handle, run));
                }
                drop(reply.send(free));
            }
            Ask::Find(handle, reply) => drop(reply.send(running.get(&handle).cloned())),
        }
    }
}

/// Stops the running run `request.run` at `request.level`; the run's own stream tells how it
/// ended.
async fn cancel(api: Arc<EngineApi>, request: wire::CancelRequest) -> Result<wire::CancelResponse, ConnectError> {
    let level = request.level();
    if level == wire::CancelLevel::Unspecified {
        return Err(ConnectError::new(Code::InvalidArgument, "say `level`: `stop` or `abandon`"));
    }
    named(&request.run)?;
    let gone = ConnectError::new(Code::NotFound, format!("no run `{}` is running", request.run));
    let run = api.runs().find(request.run).await.ok_or(gone)?;
    if level == wire::CancelLevel::Abandon {
        run.abandon();
    } else {
        run.interrupt();
    }
    Ok(wire::CancelResponse {})
}

#[cfg(test)]
#[path = "cancel_rpcs_tests.rs"]
mod tests;
