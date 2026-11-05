//! [`QuickJsHooks`], the async [`HookRunner`]: calls go over a channel to a supervisor thread,
//! which hands them one at a time to the hook thread and restarts it when a call outlives its
//! budget by more than the grace period.

use std::thread;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use vc_interfaces::{HookCall, HookError, HookOutput, HookRunner, Limit};

use crate::worker::{Answer, Outcome, Worker};

/// How long past its time budget a call may go unanswered before its thread is abandoned.
const GRACE: Duration = Duration::from_secs(1);

/// How many calls may wait for the hook thread.
const QUEUE: usize = 256;

/// One call from the async side.
struct Job {
    call: HookCall,
    reply: oneshot::Sender<Answer>,
}

/// How the supervisor runs calls.
#[derive(Clone, Copy, Debug)]
struct Settings {
    /// How long past its budget a call may run before the watchdog restarts the hook thread.
    grace: Duration,
    /// Whether the interrupt handler enforces the time budget; off only to test the watchdog.
    interrupts: bool,
}

/// Runs TypeScript hooks, already stripped to JavaScript by the compiler, in `QuickJS` on a
/// dedicated thread (D172).
///
/// Each call gets the time and memory of its [`Budget`](vc_interfaces::Budget); a call that runs
/// past either is stopped and reported with its TypeScript line, and the next call gets a fresh
/// runtime. A hook can import `vibranium:std` and, by relative path, the other hook files of
/// the program; it has no file, network, process or environment access, and reads secrets only
/// through `secret(name)`.
#[derive(Clone, Debug)]
pub struct QuickJsHooks {
    jobs: mpsc::Sender<Job>,
}

impl QuickJsHooks {
    /// A runner with its supervisor and hook threads started.
    #[must_use]
    pub fn new() -> Self {
        Self::with(Settings { grace: GRACE, interrupts: true })
    }

    fn with(settings: Settings) -> Self {
        let (jobs, inbox) = mpsc::channel(QUEUE);
        // When the thread cannot start, `inbox` is dropped and every call answers `Unavailable`.
        let _ = thread::Builder::new().name("vc-hooks-supervisor".to_owned()).spawn(move || supervise(inbox, settings));
        Self { jobs }
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

/// The supervisor thread: hands each job to the hook thread and starts a new one when it is
/// stuck or gone.
fn supervise(mut inbox: mpsc::Receiver<Job>, settings: Settings) {
    let mut worker = Worker::spawn(settings.interrupts);
    while let Some(job) = inbox.blocking_recv() {
        let (answer, lost) = attempt(&worker, job.call, settings.grace);
        if lost {
            worker = Worker::spawn(settings.interrupts);
        }
        // The caller may have gone; then nobody needs the answer.
        let _ = job.reply.send(answer);
    }
}

/// Runs `call` on `worker`; the answer, and whether the worker must be replaced.
fn attempt(worker: &Result<Worker, HookError>, call: HookCall, grace: Duration) -> (Answer, bool) {
    let worker = match worker {
        Ok(worker) => worker,
        Err(error) => return (Err(error.clone()), true),
    };
    let patience = call.budget.time.saturating_add(grace);
    match worker.run(call, patience) {
        Outcome::Done(answer) => (answer, false),
        Outcome::Stuck => (Err(HookError::Overran { limit: Limit::Time, line: None }), true),
        Outcome::Gone => (Err(HookError::Unavailable { reason: "the hook thread stopped".to_owned() }), true),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};
    use vc_interfaces::{Budget, HookCall, HookError, HookRunner, Limit};
    use vc_model::{HookModule, HookModules, HookRef};
    use vc_testkit::hook_runner_suite;

    use super::{QuickJsHooks, Settings};

    #[tokio::test]
    async fn quickjs_hooks_meet_the_hook_runner_contract() {
        hook_runner_suite(&mut || Box::new(QuickJsHooks::new()) as Box<dyn HookRunner>).await;
    }

    /// A hook stuck where the interrupt handler cannot reach it is abandoned by the watchdog,
    /// and the next call runs on a new thread.
    #[tokio::test]
    async fn the_watchdog_restarts_a_stuck_hook_thread() {
        let js = "export function stall() { const end = Date.now() + 1500; while (Date.now() < end) {} }\n\
                  export function ok() { return 'ok'; }\n";
        let modules = HookModules::new(BTreeMap::from([(
            "hooks/stall.ts".to_owned(),
            HookModule { js: js.to_owned(), lines: vec![1, 2] },
        )]));
        let call = |export: &str| {
            let hook = HookRef { file: "hooks/stall.ts".into(), export: export.into() };
            HookCall::new(modules.clone(), hook, Value::Null)
                .with_budget(Budget::new(Duration::from_millis(50), 64 << 20))
        };
        let runner = QuickJsHooks::with(Settings { grace: Duration::from_millis(100), interrupts: false });
        let started = Instant::now();
        let stalled = runner.call(call("stall")).await;
        assert_eq!(stalled, Err(HookError::Overran { limit: Limit::Time, line: None }));
        assert!(started.elapsed() < Duration::from_millis(1000), "the watchdog did not wait for the hook");
        assert_eq!(runner.call(call("ok")).await.map(|output| output.value), Ok(json!("ok")));
    }
}
