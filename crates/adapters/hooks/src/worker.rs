//! The hook thread, which owns the `QuickJS` engine, and the handle the supervisor drives it by.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use vc_interfaces::{HookCall, HookError, HookOutput};

use crate::engine::Engine;

/// What a call gives back.
pub type Answer = Result<HookOutput, HookError>;

/// One call for the hook thread, with where to say it has taken the call up and where to send
/// its answer.
struct Task {
    call: HookCall,
    started: Sender<()>,
    reply: Sender<Answer>,
}

/// How a call on the hook thread went.
pub enum Outcome {
    /// It answered in time.
    Done(Answer),
    /// It did not answer in time; the thread is abandoned.
    Stuck,
    /// The thread is gone.
    Gone,
}

/// A running hook thread. Dropping it lets the thread end once its current call returns.
pub struct Worker {
    tasks: Sender<Task>,
    /// How long the thread may take to take a call up before it counts as stuck.
    start: Duration,
    /// The hook thread, to wait for when it is retired.
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts a hook thread; `interrupts` arms the time budget, and a call it has not taken up
    /// within `start` is [`Outcome::Stuck`].
    ///
    /// # Errors
    ///
    /// [`HookError::Unavailable`] when the thread cannot start.
    pub fn spawn(interrupts: bool, start: Duration) -> Result<Self, HookError> {
        let (tasks, inbox) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("vc-hooks".to_owned())
            .spawn(move || serve(&inbox, interrupts))
            .map_err(|error| HookError::Unavailable { reason: format!("the hook thread could not start: {error}") })?;
        Ok(Self { tasks, start, thread: Some(thread) })
    }

    /// Ends the hook thread and waits for it; only for a worker with no call in flight, which a
    /// stuck one may still have.
    pub fn retire(self) {
        drop(self.tasks);
        if let Some(thread) = self.thread {
            // A hook thread that panicked has ended all the same.
            let _ = thread.join();
        }
    }

    /// Runs `call`, waiting at most `patience` for its answer once the hook thread has taken it
    /// up: starting a fresh engine, or waiting to be scheduled on a loaded machine, uses none of
    /// it. A thread that has not taken the call up within the start bound is stuck; one that
    /// could not (its engine did not start) has answered already, or is gone.
    pub fn run(&self, call: HookCall, patience: Duration) -> Outcome {
        let ((started, begun), (reply, answer)) = (mpsc::channel(), mpsc::channel());
        if self.tasks.send(Task { call, started, reply }).is_err() {
            return Outcome::Gone;
        }
        if matches!(begun.recv_timeout(self.start), Err(RecvTimeoutError::Timeout)) {
            return Outcome::Stuck;
        }
        match answer.recv_timeout(patience) {
            Ok(answer) => Outcome::Done(answer),
            Err(RecvTimeoutError::Timeout) => Outcome::Stuck,
            Err(RecvTimeoutError::Disconnected) => Outcome::Gone,
        }
    }
}

/// The hook thread: runs each task in turn until its handle is dropped.
fn serve(inbox: &Receiver<Task>, interrupts: bool) {
    let mut engine = None;
    while let Ok(task) = inbox.recv() {
        let answer = run(&mut engine, &task, interrupts);
        // The supervisor may have stopped waiting; then nobody needs the answer.
        let _ = task.reply.send(answer);
    }
}

/// Runs the task's call on the engine, starting one when there is none and dropping it after an
/// overrun so the next call gets a clean runtime; says when the call starts.
fn run(engine: &mut Option<Engine>, task: &Task, interrupts: bool) -> Answer {
    let mut ready = match engine.take() {
        Some(ready) => ready,
        None => Engine::new(interrupts)
            .map_err(|error| HookError::Unavailable { reason: format!("QuickJS could not start: {error}") })?,
    };
    // The supervisor waits for this before it starts its watch.
    let _ = task.started.send(());
    let answer = ready.call(&task.call);
    if !matches!(answer, Err(HookError::Overran { .. })) {
        *engine = Some(ready);
    }
    answer
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};
    use std::thread;
    use std::time::{Duration, Instant};

    use serde_json::Value;
    use vc_interfaces::{HookCall, HookError};
    use vc_model::{HookModules, HookRef};

    use super::{Outcome, Task, Worker};

    /// A worker whose calls go to `inbox`, which the test serves, or holds and never serves.
    fn worker(start: Duration) -> (Worker, Receiver<Task>) {
        let (tasks, inbox) = mpsc::channel();
        (Worker { tasks, start, thread: None }, inbox)
    }

    fn call() -> HookCall {
        HookCall::new(HookModules::default(), HookRef { file: "hooks/a.ts".into(), export: "a".into() }, Value::Null)
    }

    /// A hung engine start or a thread that never gets scheduled: the call is stuck after the
    /// start bound, not after its patience and not never.
    #[test]
    fn a_call_the_thread_never_takes_up_is_stuck_after_the_start_bound() {
        let (worker, _held) = worker(Duration::from_millis(50));
        let asked = Instant::now();
        assert!(matches!(worker.run(call(), Duration::from_secs(600)), Outcome::Stuck));
        assert!(asked.elapsed() < Duration::from_secs(600), "the patience never began");
    }

    /// An engine that could not start answers with its error and never says it started.
    #[test]
    fn a_thread_that_answers_without_starting_gives_its_answer() {
        let (worker, inbox) = worker(Duration::from_secs(600));
        let unavailable = HookError::Unavailable { reason: "QuickJS could not start: x".to_owned() };
        let sent = unavailable.clone();
        let thread = thread::spawn(move || inbox.recv().map(|task| task.reply.send(Err(sent))));
        let outcome = worker.run(call(), Duration::from_secs(600));
        assert!(matches!(outcome, Outcome::Done(Err(error)) if error == unavailable));
        thread.join().unwrap().unwrap().unwrap();
    }

    /// A thread that drops the call without starting or answering it is gone.
    #[test]
    fn a_thread_that_drops_the_call_is_gone() {
        let (worker, inbox) = worker(Duration::from_secs(600));
        let thread = thread::spawn(move || drop(inbox.recv()));
        assert!(matches!(worker.run(call(), Duration::from_secs(600)), Outcome::Gone));
        thread.join().unwrap();
    }
}
