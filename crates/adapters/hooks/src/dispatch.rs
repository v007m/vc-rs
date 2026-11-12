//! The dispatcher and its supervisors (D325): the dispatcher hands each call to a supervisor
//! that has said it is idle, and each supervisor runs its calls on its own hook thread, which it
//! replaces when a call outlives its budget by more than the grace period.

use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tokio::sync::{mpsc as queue, oneshot};
use vc_interfaces::{HookCall, HookError, Limit};

use crate::worker::{Answer, Outcome, Worker};

/// One call from the async side.
pub struct Job {
    pub call: HookCall,
    pub reply: oneshot::Sender<Answer>,
}

/// How the supervisors run calls.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// How long past its budget a call may run before the watchdog restarts the hook thread.
    pub grace: Duration,
    /// How long the hook thread may take to take a call up before it is restarted.
    pub start: Duration,
    /// Whether the interrupt handler enforces the time budget; off only to test the watchdog.
    pub interrupts: bool,
}

/// A supervisor as the dispatcher sees it: where its calls go, and its thread.
struct Lane {
    jobs: Sender<Job>,
    thread: JoinHandle<()>,
}

/// The dispatcher thread: starts `threads` supervisors, then hands each job from `inbox` to one
/// that has said it is idle, so no call waits behind a busy or stuck runtime while another is
/// free. Once `inbox` closes it closes every lane and waits for the supervisors to end.
pub fn dispatch(mut inbox: queue::Receiver<Job>, settings: Settings, threads: u32) {
    let (ready, idle) = mpsc::channel();
    let mut lanes = Vec::new();
    for _ in 0..threads {
        if let Some(lane) = Lane::spawn(lanes.len(), settings, ready.clone()) {
            lanes.push(lane);
        }
    }
    // Only the supervisors hold `ready` now: when none started, `idle` says so at once.
    drop(ready);
    while let Some(job) = inbox.blocking_recv() {
        hand(&lanes, &idle, job);
    }
    let threads: Vec<JoinHandle<()>> = lanes.into_iter().map(|lane| lane.thread).collect();
    for thread in threads {
        // A supervisor that panicked has ended all the same.
        let _ = thread.join();
    }
}

/// Gives `job` to the next idle supervisor, or answers it `Unavailable` when none is left.
fn hand(lanes: &[Lane], idle: &Receiver<usize>, job: Job) {
    let Some(lane) = idle.recv().ok().and_then(|index| lanes.get(index)) else { return refuse(job) };
    if let Err(SendError(job)) = lane.jobs.send(job) {
        refuse(job);
    }
}

fn refuse(job: Job) {
    // The caller may have gone; then nobody needs the answer.
    let _ = job.reply.send(Err(HookError::Unavailable { reason: "the hook thread is not running".to_owned() }));
}

impl Lane {
    /// Starts supervisor `index`, which says on `ready` when it is idle; none when its thread
    /// cannot start.
    fn spawn(index: usize, settings: Settings, ready: Sender<usize>) -> Option<Self> {
        let (jobs, inbox) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("vc-hooks-supervisor".to_owned())
            .spawn(move || supervise(&inbox, settings, (index, &ready)))
            .ok()?;
        Some(Self { jobs, thread })
    }
}

/// A supervisor thread: says it is idle, runs the job it is handed on its hook thread, and
/// starts a new one when it is stuck or gone before it says it is idle again. Once its lane
/// closes it stops its hook thread and waits for it.
fn supervise(inbox: &Receiver<Job>, settings: Settings, (index, ready): (usize, &Sender<usize>)) {
    let mut worker = Worker::spawn(settings.interrupts, settings.start);
    while ready.send(index).is_ok() {
        let Ok(job) = inbox.recv() else { break };
        let (answer, lost) = attempt(&worker, job.call, settings.grace);
        if lost {
            worker = Worker::spawn(settings.interrupts, settings.start);
        }
        // The caller may have gone; then nobody needs the answer.
        let _ = job.reply.send(answer);
    }
    if let Ok(worker) = worker {
        worker.retire();
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

    use serde_json::Value;
    use vc_model::{HookModules, HookRef};

    use super::*;

    /// With no supervisor left to say it is idle, a job is answered at once, not dropped.
    #[test]
    fn a_job_with_no_supervisor_left_is_answered_unavailable() {
        let (reply, answer) = oneshot::channel();
        let hook = HookRef { file: "hooks/a.ts".into(), export: "a".into() };
        let call = HookCall::new(HookModules::new(BTreeMap::new()), hook, Value::Null);
        let (_, idle) = mpsc::channel::<usize>();
        hand(&[], &idle, Job { call, reply });
        let reason = "the hook thread is not running".to_owned();
        assert_eq!(answer.blocking_recv().ok(), Some(Err(HookError::Unavailable { reason })));
    }
}
