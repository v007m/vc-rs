use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{self, Instant};
use vc_diagnostics::Code;
use vc_model::{ActionId, ActionOutcome, CleanupRecord, Event, Failure, OwnerKey};

use super::script::Scripts;
use crate::cancel::{CancelTree, Ran};
use crate::{
    ActionCx, ActionReport, ActionRequest, CANCELLED, CleanupCollector, Frame, PendingCleanup, RedactingSink, RunInput,
    Saved, SharedSlots,
};

/// What every worker of a run shares.
#[derive(Clone)]
pub struct Crew {
    /// The program and the action executors.
    pub cx: Arc<ActionCx>,
    /// Where events go, masked by the run's redactor.
    pub bus: RedactingSink,
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
    pub frame: Frame,
    /// When it must be done by; cleanups have none.
    pub deadline: Option<Deadline>,
}

/// When a job must be done by, and what its V5104 failure says it ran past.
#[derive(Clone, Copy, Debug)]
pub struct Deadline {
    pub at: Instant,
    pub past: &'static str,
}

impl Deadline {
    /// An attempt's: the test's `timeout` from now.
    pub fn attempt(timeout: Duration) -> Self {
        Self { at: Instant::now() + timeout, past: "the test ran past its `timeout`" }
    }

    /// An `after` or `afterEach` action's own, `limit` from now (D192).
    pub fn after(limit: Duration) -> Self {
        Self { at: Instant::now() + limit, past: "the `after` action ran past its own `timeout`" }
    }
}

impl Job {
    /// The slots a shared action of this frame fills; instance actions fill none.
    fn slots(&self) -> Option<Arc<SharedSlots>> {
        match self.owner {
            OwnerKey::Run => Some(Arc::clone(&self.frame.run)),
            OwnerKey::File(_) => Some(Arc::clone(&self.frame.file)),
            OwnerKey::Instance(_) => None,
        }
    }

    /// The same job running as a cleanup with `vars`.
    pub fn cleaning(self, vars: &Arc<Saved>) -> Self {
        Self { frame: self.frame.cleaning(vars), ..self }
    }

    fn request(self, cx: &Arc<ActionCx>) -> ActionRequest {
        ActionRequest { action: self.action, owner: self.owner, cx: Arc::clone(cx), frame: self.frame }
    }
}

impl Crew {
    /// Runs an action under `cancel`, with its started, check and finished events. A shared
    /// action's result fills its slot. Cut off or aborted, it gives back the cleanups its
    /// finished parts registered; done, its report holds them.
    pub async fn act(&self, job: Job, cancel: &CancelTree) -> (Ran<ActionReport>, Vec<PendingCleanup>) {
        let (action, owner, slots, deadline) = (job.action, job.owner, job.slots(), job.deadline);
        self.bus.publish(Event::ActionStarted { action, owner }).await;
        let work = |request| cancel.guard(self.execute(request, deadline));
        let (ran, kept) = merged(CleanupCollector::gather(&job.request(&self.cx), work).await);
        let outcome = match &ran {
            Ran::Done(report) => {
                self.publish_report((action, owner), report).await;
                if let Some(slots) = slots {
                    let _first = slots.fill(action, report);
                }
                report.outcome.clone()
            }
            Ran::Cut | Ran::Aborted => ActionOutcome::Failed(Failure::new(CANCELLED).at(action)),
        };
        self.bus.publish(Event::ActionFinished { action, owner, outcome }).await;
        (ran, kept)
    }

    /// Publishes what `report` holds for the action: its exchanges, checks and warnings.
    async fn publish_report(&self, (action, owner): (ActionId, OwnerKey), report: &ActionReport) {
        for exchange in &report.exchanges {
            self.bus.publish(Event::Exchanged { action, owner, exchange: Box::new(exchange.clone()) }).await;
        }
        for check in &report.checks {
            self.bus.publish(Event::CheckEvaluated { action, owner, check: Box::new(check.clone()) }).await;
        }
        for warning in &report.warnings {
            self.bus.publish(Event::Warned { action, owner, warning: warning.clone() }).await;
        }
    }

    /// Runs `request`'s action; past `deadline` it is dropped and fails with V5104, the
    /// cleanups of its finished parts handed to the frame's sink.
    async fn execute(&self, request: ActionRequest, deadline: Option<Deadline>) -> ActionReport {
        let action = request.action;
        let work = self.cx.executors.execute(request);
        let Some(deadline) = deadline else { return work.await };
        time::timeout_at(deadline.at, work).await.unwrap_or_else(|_| {
            let failure = Failure::new(deadline.past).with_code(Code::V5104.to_string());
            ActionReport::failed(failure.at(action))
        })
    }

    /// Runs a cleanup: cancellation does not stop it, an abort does.
    pub async fn clean(&self, job: Job, cancel: &CancelTree) -> Ran<ActionReport> {
        let (action, owner) = (job.action, job.owner);
        let ran = cancel.cleanup(self.cx.executors.execute(job.request(&self.cx))).await;
        if let Ran::Done(report) = &ran {
            let cleanup = CleanupRecord { action, owner, outcome: report.outcome.clone() };
            self.bus.publish(Event::CleanupFinished { cleanup }).await;
        }
        ran
    }
}

/// A done action's report with the `kept` cleanups first and nothing left over; otherwise the
/// cleanups `kept` are left over.
fn merged((ran, mut kept): (Ran<ActionReport>, Vec<PendingCleanup>)) -> (Ran<ActionReport>, Vec<PendingCleanup>) {
    let Ran::Done(mut report) = ran else { return (ran, kept) };
    kept.append(&mut report.cleanups);
    report.cleanups = kept;
    (Ran::Done(report), Vec::new())
}
