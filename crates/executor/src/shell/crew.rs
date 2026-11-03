use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedSender;
use vc_model::{ActionId, ActionOutcome, CleanupRecord, Event, Failure, OwnerKey};

use super::script::Scripts;
use crate::cancel::{CancelTree, Ran};
use crate::{ActionCx, ActionReport, ActionRequest, CANCELLED, EventBus, RunInput, Scope, SharedSlots};

/// What every worker of a run shares.
#[derive(Clone)]
pub struct Crew {
    /// The program and the action executors.
    pub cx: Arc<ActionCx>,
    /// Where events go.
    pub bus: EventBus,
    /// Each planned test's actions.
    pub scripts: Arc<Scripts>,
    /// Bounds the instances running at once, across files.
    pub limit: Arc<Semaphore>,
    /// How many instances a file launches at once.
    pub window: usize,
    /// The run's inbox, for test verdicts (`--bail`).
    pub run: UnboundedSender<RunInput>,
}

/// One action to run, in its frame.
pub struct Job {
    pub action: ActionId,
    pub owner: OwnerKey,
    pub scope: Scope,
}

impl Job {
    /// The slots a shared action of this frame fills; instance actions fill none.
    fn slots(&self) -> Option<Arc<SharedSlots>> {
        match self.owner {
            OwnerKey::Run => Some(Arc::clone(&self.scope.run)),
            OwnerKey::File(_) => Some(Arc::clone(&self.scope.file)),
            OwnerKey::Instance(_) => None,
        }
    }

    fn request(self, cx: &Arc<ActionCx>) -> ActionRequest {
        ActionRequest { action: self.action, owner: self.owner, cx: Arc::clone(cx), scope: self.scope }
    }
}

impl Crew {
    /// Runs an action under `cancel`, with its started and finished events. A shared action's
    /// result fills its slot.
    pub async fn act(&self, job: Job, cancel: &CancelTree) -> Ran<ActionReport> {
        let (action, owner, slots) = (job.action, job.owner, job.slots());
        self.bus.publish(Event::ActionStarted { action, owner });
        let ran = cancel.guard(self.cx.executors.execute(job.request(&self.cx))).await;
        let outcome = match &ran {
            Ran::Done(report) => {
                if let Some(slots) = slots {
                    slots.fill(action, report);
                }
                report.outcome.clone()
            }
            Ran::Cut | Ran::Aborted => ActionOutcome::Failed(Failure::new(CANCELLED).at(action)),
        };
        self.bus.publish(Event::ActionFinished { action, owner, outcome });
        ran
    }

    /// Runs a cleanup: cancellation does not stop it, an abort does.
    pub async fn clean(&self, job: Job, cancel: &CancelTree) -> Ran<ActionReport> {
        let (action, owner) = (job.action, job.owner);
        let ran = cancel.cleanup(self.cx.executors.execute(job.request(&self.cx))).await;
        if let Ran::Done(report) = &ran {
            let cleanup = CleanupRecord { action, owner, outcome: report.outcome.clone() };
            self.bus.publish(Event::CleanupFinished { cleanup });
        }
        ran
    }
}
