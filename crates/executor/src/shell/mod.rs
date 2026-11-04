//! The async shells that carry out the machines' commands. Each frame (run, file, instance) is
//! one task that owns its machine; its workers report back as inputs (orchestration, D103).

mod crew;
mod file;
mod instance;
mod run;
mod script;
mod tasks;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::JoinHandle;
use vc_model::Plan;

use self::crew::Crew;
use self::run::RunFrame;
use crate::cancel::CancelTree;
use crate::{ActionCx, EventBus, ExecutorError, RedactingSink, RunInput, RunSummary};

/// How a run is carried out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct RunOptions {
    /// The most test instances running at once, across files (`--concurrency`).
    pub concurrency: usize,
    /// How long work in flight may finish after the first Ctrl-C.
    pub grace: Duration,
    /// Stop starting tests after the first failure (`--bail`).
    pub bail: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self { concurrency: 16, grace: Duration::from_secs(5), bail: false }
    }
}

/// Runs plans of one program with one set of action executors.
#[derive(Clone)]
pub struct Executor {
    cx: Arc<ActionCx>,
    options: RunOptions,
}

impl std::fmt::Debug for Executor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Executor").field("executors", &self.cx.executors).field("options", &self.options).finish()
    }
}

impl Executor {
    /// An executor for the program, executors and run values of `cx`.
    #[must_use]
    pub const fn new(cx: Arc<ActionCx>, options: RunOptions) -> Self {
        Self { cx, options }
    }

    /// Starts running `plan`, publishing its events on `bus`. Call it from within a Tokio
    /// runtime.
    ///
    /// # Errors
    ///
    /// [`ExecutorError::UnknownTest`], [`ExecutorError::UnknownAction`] or
    /// [`ExecutorError::NoExecutor`] when the plan reaches something this build cannot run;
    /// nothing has started then.
    pub fn start(&self, plan: Arc<Plan>, bus: EventBus) -> Result<RunHandle, ExecutorError> {
        let scripts = script::scripts(&self.cx.program, &plan)?;
        self.cx.executors.check(&self.cx.program, script::roots(&plan, &scripts))?;
        let (inbox, inputs) = mpsc::unbounded_channel();
        let concurrency = self.options.concurrency.max(1);
        let crew = Crew {
            cx: Arc::clone(&self.cx),
            bus: RedactingSink::new(bus, Arc::clone(&self.cx.redactor)),
            scripts: Arc::new(scripts),
            limit: Arc::new(Semaphore::new(concurrency)),
            window: concurrency,
            run: inbox.clone(),
        };
        let frame = RunFrame { inbox: inputs, cancel: CancelTree::new(self.options.grace), bail: self.options.bail };
        Ok(RunHandle { inbox, task: tokio::spawn(run::run(crew, plan, frame)) })
    }
}

/// A run in progress.
#[derive(Debug)]
pub struct RunHandle {
    inbox: UnboundedSender<RunInput>,
    task: JoinHandle<RunSummary>,
}

impl RunHandle {
    /// A Ctrl-C: the first cancels (work in flight gets the grace period, cleanups run), the
    /// second ends the grace period, the third aborts (cleanups are abandoned).
    pub fn interrupt(&self) {
        self.inbox.send(RunInput::Interrupt).ok();
    }

    /// Waits for the run to finish.
    ///
    /// # Errors
    ///
    /// [`ExecutorError::Stopped`] when the run task ended without a summary.
    pub async fn finish(self) -> Result<RunSummary, ExecutorError> {
        self.task.await.map_err(|_| ExecutorError::Stopped)
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "flow_tests.rs"]
mod flow_tests;
