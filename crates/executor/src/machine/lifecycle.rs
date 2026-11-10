use vc_model::{ActionId, OwnerKey, Plan};

/// One step of a load run's shared lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleStep {
    /// Run this shared action of its owner; skipped once the run is cancelled.
    Act(OwnerKey, ActionId),
    /// Run the cleanups the owner's actions registered, newest first; only an abort stops them.
    Clean(OwnerKey),
}

impl LifecycleStep {
    /// Whether the step runs once the run is `cancelled` or not: cleanups always do.
    pub const fn runs(self, cancelled: bool) -> bool {
        matches!(self, Self::Clean(_)) || !cancelled
    }
}

/// The order of a load run's shared lifecycle (D122), decided from the plan alone, so the
/// load shell only carries it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadLifecycle {
    /// Before the first stage: `beforeRun`, then each file's `beforeAll`, in plan order, with
    /// their owners. The first that fails or is cut stops the rest, the load run does not start
    /// and [`LoadLifecycle::unwind`] runs.
    pub prepare: Vec<(OwnerKey, ActionId)>,
    /// Once the load run is over: each file's `afterAll` and its cleanups, then `afterRun` and
    /// the run's.
    pub finish: Vec<LifecycleStep>,
}

impl LoadLifecycle {
    pub fn new(plan: &Plan) -> Self {
        let mut prepare: Vec<_> = plan.before_run.iter().map(|planned| (OwnerKey::Run, planned.action)).collect();
        let mut finish = Vec::new();
        for file in &plan.files {
            let owner = OwnerKey::File(file.id);
            prepare.extend(file.before_all.iter().map(|planned| (owner, planned.action)));
            finish.extend(file.after_all.iter().map(|&action| LifecycleStep::Act(owner, action)));
            finish.push(LifecycleStep::Clean(owner));
        }
        finish.extend(plan.after_run.iter().map(|&action| LifecycleStep::Act(OwnerKey::Run, action)));
        finish.push(LifecycleStep::Clean(OwnerKey::Run));
        Self { prepare, finish }
    }

    /// After a failed `prepare`: only the cleanups of `finish`, in its order.
    pub fn unwind(&self) -> impl Iterator<Item = LifecycleStep> + '_ {
        self.finish.iter().copied().filter(|step| matches!(step, LifecycleStep::Clean(_)))
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
