use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use vc_model::{ActionId, Event, FileId, Instance, OwnerKey, Plan, PlannedAction, TestId};

use super::crew::{Crew, Job};
use super::instance::{self, Ticket};
use super::script;
use crate::cancel::{CancelTree, Ran};
use crate::machine::{LifecycleStep, LoadLifecycle};
use crate::{
    ActionCx, CANCELLED, EventBus, ExecutorError, Frame, IterationFailure, IterationReport, Iterations, Launch,
    PendingCleanup, RedactingSink, SharedSlots,
};

/// What a load run's iterations share besides the program and the plan.
#[derive(Clone, Debug)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the engine and by executor tests; every user is in this repo (D160)"
)]
pub struct IterationSetup {
    /// Where the actions' events go, masked by the run's redactor.
    pub bus: EventBus,
    /// The run's root (Ctrl-C): cancelling it cuts the iterations; `afterAll` and `afterRun`
    /// are skipped then, cleanups still run.
    pub cancel: CancelTree,
    /// `checks: full`: each call judges the test's and the step's checks too, not only its
    /// api's own `expect`.
    pub full_checks: bool,
}

/// The program's tests as load iterations.
///
/// `beforeRun` and each `beforeAll` run once, before the first iteration; each iteration is one
/// instance of its test with those shared results. [`ProgramIterations::finish`] runs `afterAll`, `afterRun` and the shared cleanups once the
/// load run is over. The order is [`LoadLifecycle`]'s; this shell only carries it out.
pub struct ProgramIterations {
    crew: Crew,
    lifecycle: LoadLifecycle,
    run: Frame,
    files: BTreeMap<FileId, Frame>,
    /// Each test's first planned instance, with its file.
    instances: BTreeMap<TestId, (Instance, FileId)>,
    /// The cleanups the shared actions registered, oldest first, by owner.
    cleanups: BTreeMap<OwnerKey, Vec<PendingCleanup>>,
    cancel: CancelTree,
    /// The load run's frame, a child of the root: an `abortOn` breach cancels only the users.
    users: CancelTree,
    full_checks: bool,
}

impl std::fmt::Debug for ProgramIterations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgramIterations")
            .field("tests", &self.instances.keys().collect::<Vec<_>>())
            .field("full_checks", &self.full_checks)
            .finish_non_exhaustive()
    }
}

impl ProgramIterations {
    /// Runs `plan`'s `beforeRun`, then each file's `beforeAll`, in order, once.
    ///
    /// # Errors
    ///
    /// [`ExecutorError::UnknownTest`], [`ExecutorError::UnknownAction`] or
    /// [`ExecutorError::NoExecutor`] when the plan reaches something this build cannot run, and
    /// [`ExecutorError::SharedFailed`] when a shared action fails or is cut; the cleanups its
    /// earlier actions registered have run then.
    pub async fn prepare(cx: Arc<ActionCx>, plan: Arc<Plan>, setup: IterationSetup) -> Result<Self, ExecutorError> {
        let scripts = script::scripts(&cx.program, &plan)?;
        cx.executors.check(&cx.program, script::roots(&plan, &scripts))?;
        let crew = Crew {
            bus: RedactingSink::new(setup.bus.clone(), Arc::clone(&cx.redactor)),
            cx,
            scripts: Arc::new(scripts),
            limit: Arc::new(Semaphore::new(Semaphore::MAX_PERMITS)),
            window: 1,
            run: mpsc::unbounded_channel().0,
        };
        let mut iterations = Self::new(crew, &plan, setup);
        if let Err(error) = iterations.share().await {
            for step in iterations.lifecycle.unwind() {
                iterations.carry_out(step).await;
            }
            return Err(error);
        }
        Ok(iterations)
    }

    fn new(crew: Crew, plan: &Plan, setup: IterationSetup) -> Self {
        let slots =
            |actions: &[PlannedAction]| Arc::new(SharedSlots::new(actions.iter().map(|planned| planned.action)));
        let run = Frame { run: slots(&plan.before_run), ..Frame::default() };
        let mut instances = BTreeMap::new();
        let mut files = BTreeMap::new();
        for file in &plan.files {
            files.insert(
                file.id,
                Frame { run: Arc::clone(&run.run), file: slots(&file.before_all), ..Frame::default() },
            );
            for instance in &file.instances {
                instances.entry(instance.test).or_insert_with(|| (instance.clone(), file.id));
            }
        }
        let users = setup.cancel.child();
        let (lifecycle, cleanups) = (LoadLifecycle::new(plan), BTreeMap::new());
        Self {
            crew,
            lifecycle,
            run,
            files,
            instances,
            cleanups,
            cancel: setup.cancel,
            users,
            full_checks: setup.full_checks,
        }
    }

    /// The frame to run the load under: cancelling it cuts every iteration in flight.
    #[must_use]
    pub fn users(&self) -> CancelTree {
        self.users.clone()
    }

    /// Runs the lifecycle's shared actions in its order, keeping their cleanups; the first that
    /// fails or is cut stops them.
    async fn share(&mut self) -> Result<(), ExecutorError> {
        for (owner, action) in self.lifecycle.prepare.clone() {
            let (ran, kept) = self.crew.act(self.job(owner, action), &self.cancel).await;
            let (cleanups, failure) = match ran {
                Ran::Done(report) => (report.cleanups, report.outcome.failure().map(|failure| failure.message.clone())),
                Ran::Cut | Ran::Aborted => (kept, Some(CANCELLED.to_owned())),
            };
            self.cleanups.entry(owner).or_default().extend(cleanups);
            if let Some(message) = failure {
                return Err(ExecutorError::SharedFailed { action, message });
            }
        }
        Ok(())
    }

    /// Once the load run is over, the lifecycle's `finish` steps in its order; once the root is
    /// cancelled only the cleanups run, and an abort stops those too.
    pub async fn finish(&self) {
        for &step in &self.lifecycle.finish {
            if step.runs(self.cancel.is_cancelled()) {
                self.carry_out(step).await;
            }
        }
    }

    async fn carry_out(&self, step: LifecycleStep) {
        match step {
            LifecycleStep::Act(owner, action) => {
                let _ran = self.crew.act(self.job(owner, action), &self.cancel).await;
            }
            LifecycleStep::Clean(owner) => self.clean(owner).await,
        }
    }

    /// `owner`'s cleanups, newest first, until an abort: then the one it stopped and those left
    /// are abandoned, in that order.
    async fn clean(&self, owner: OwnerKey) {
        let cleanups = self.cleanups.get(&owner).map_or(&[][..], Vec::as_slice);
        for (index, cleanup) in cleanups.iter().enumerate().rev() {
            let job = self.job(owner, cleanup.action).cleaning(&cleanup.vars);
            if self.crew.clean(job, &self.cancel).await == Ran::Aborted {
                let cleanups = cleanups[..=index].iter().rev().map(|cleanup| cleanup.action).collect();
                return self.crew.bus.publish(Event::CleanupsAbandoned { owner, cleanups }).await;
            }
        }
    }

    /// `action` in `owner`'s frame.
    fn job(&self, owner: OwnerKey, action: ActionId) -> Job {
        let frame = match owner {
            OwnerKey::File(file) => self.files.get(&file).cloned().unwrap_or_default(),
            _ => self.run.clone(),
        };
        Job { action, owner, frame, deadline: None }
    }
}

#[async_trait]
impl Iterations for ProgramIterations {
    async fn iterate(&self, test: TestId, _user: u32) -> IterationReport {
        let Some((instance, file)) = self.instances.get(&test) else {
            return IterationReport {
                failure: Some(IterationFailure {
                    message: "the test is not in the load plan".to_owned(),
                    ..Default::default()
                }),
                ..IterationReport::default()
            };
        };
        let file = self.files.get(file).cloned().unwrap_or_default();
        let frame = Frame { api_checks_only: !self.full_checks, ..file };
        let launch = Launch { instance: instance.clone(), block: None };
        let ticket = Ticket { launch, frame, cancel: self.users.child() };
        instance::iteration(self.crew.clone(), ticket).await
    }
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
