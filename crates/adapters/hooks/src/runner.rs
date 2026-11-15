//! [`QuickJsHooks`], the async [`HookRunner`]: calls go over a channel to the dispatcher thread,
//! which hands each to an idle one of its supervisors (D325).

use std::thread::{self, JoinHandle};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use vc_interfaces::{HookCall, HookError, HookOutput, HookRunner};
use vc_model::HooksConfig;

use crate::dispatch::{Job, Settings, dispatch};

/// How long past its time budget a call may go unanswered before its thread is abandoned.
const GRACE: Duration = Duration::from_secs(1);

/// How long the hook thread may take to take a call up: start `QuickJS` and get scheduled. A
/// fresh runtime starts in milliseconds, so this is for a thread that will never start the call;
/// it is generous so a loaded machine is not mistaken for one.
const START: Duration = Duration::from_secs(10);

/// How many calls may wait for a hook thread.
const QUEUE: usize = 256;

/// Runs TypeScript hooks, already stripped to JavaScript by the compiler, in `QuickJS` on a
/// dedicated thread, or on `hooks.threads` of them (D172, D325).
///
/// Each call gets the time and memory of its [`Budget`](vc_interfaces::Budget); a call that runs
/// past either is stopped and reported with its TypeScript line, and the next call gets a fresh
/// runtime. A hook can import `vibranium:std` and, by relative path, the other hook files of
/// the program; it has no file, network, process or environment access, and reads secrets only
/// through `secret(name)`. Module state is per runtime: with more than one, a call may land on
/// any of them, so a hook must not rely on module-level state across calls.
#[derive(Clone, Debug)]
pub struct QuickJsHooks {
    jobs: mpsc::Sender<Job>,
}

impl QuickJsHooks {
    /// A runner with one hook runtime, its dispatcher, supervisor and hook threads started.
    #[must_use]
    pub fn new() -> Self {
        Self::with_threads(1)
    }

    /// A runner with `threads` hook runtimes, each on its own thread with its own module state,
    /// so that many calls run at once (`hooks.threads`, D325); `threads` is held to 1 through
    /// [`HooksConfig::MAX_THREADS`].
    #[must_use]
    pub fn with_threads(threads: u32) -> Self {
        Self::start(Settings { grace: GRACE, start: START, interrupts: true }, threads).0
    }

    /// The runner and its dispatcher thread, which ends, once every clone of the runner is
    /// dropped, after its supervisor and hook threads; none when it could not start.
    fn start(settings: Settings, threads: u32) -> (Self, Option<JoinHandle<()>>) {
        let threads = threads.clamp(1, HooksConfig::MAX_THREADS);
        let (jobs, inbox) = mpsc::channel(QUEUE);
        // When the thread cannot start, `inbox` is dropped and every call answers `Unavailable`.
        let dispatcher = thread::Builder::new()
            .name("vc-hooks-dispatcher".to_owned())
            .spawn(move || dispatch(inbox, settings, threads))
            .ok();
        (Self { jobs }, dispatcher)
    }
}

impl Default for QuickJsHooks {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HookRunner for QuickJsHooks {
    async fn call(&self, call: HookCall) -> Result<HookOutput, HookError> {
        let stopped = || HookError::Unavailable { reason: "the hook thread is not running".to_owned() };
        let (reply, answer) = oneshot::channel();
        self.jobs.send(Job { call, reply }).await.map_err(|_| stopped())?;
        answer.await.map_err(|_| stopped())?
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
