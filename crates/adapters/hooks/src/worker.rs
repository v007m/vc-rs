//! The hook thread, which owns the `QuickJS` engine, and the handle the supervisor drives it by.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
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
}

impl Worker {
    /// Starts a hook thread; `interrupts` arms the time budget.
    ///
    /// # Errors
    ///
    /// [`HookError::Unavailable`] when the thread cannot start.
    pub fn spawn(interrupts: bool) -> Result<Self, HookError> {
        let (tasks, inbox) = mpsc::channel();
        thread::Builder::new()
            .name("vc-hooks".to_owned())
            .spawn(move || serve(&inbox, interrupts))
            .map_err(|error| HookError::Unavailable { reason: format!("the hook thread could not start: {error}") })?;
        Ok(Self { tasks })
    }

    /// Runs `call`, waiting at most `patience` for its answer once the hook thread has taken it
    /// up: starting a fresh engine, or waiting to be scheduled on a loaded machine, uses none of
    /// it. A thread that cannot take the call up has answered already, or is gone.
    pub fn run(&self, call: HookCall, patience: Duration) -> Outcome {
        let ((started, begun), (reply, answer)) = (mpsc::channel(), mpsc::channel());
        if self.tasks.send(Task { call, started, reply }).is_err() {
            return Outcome::Gone;
        }
        let _ = begun.recv();
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
