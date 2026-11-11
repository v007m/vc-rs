//! The pure state machines that decide a run: [`RunOrchestrator`], [`FileOrchestrator`] and
//! [`InstanceWorkflow`]. Each is `transition(state, input) -> (state, commands)` with no I/O, no
//! clock and no async; the shells in `crate::shell` carry out the commands and feed back inputs.
//! A load run's shared lifecycle is a fixed order, so [`LoadLifecycle`] gives it as a list.

mod file;
mod instance;
mod lifecycle;
mod run;

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use vc_model::{ActionId, ActionOutcome, CheckResult, Event, Exchange, Failure, Needs, OwnerKey, PlannedAction};

pub use file::{FileCommand, FileInput, FileOrchestrator, FilePhase, Launch};
pub use instance::{
    CANCELLED, InstanceCommand, InstanceInput, InstancePhase, InstanceScript, InstanceWorkflow, Verdict,
};
pub use lifecycle::{LifecycleStep, LoadLifecycle};
pub use run::{RunCommand, RunInput, RunOrchestrator, RunPhase, RunSummary};

/// Values an action saved, by name.
pub type Saved = serde_json::Map<String, serde_json::Value>;

/// The shared lifecycle actions that failed, with their failures; instances and later shared
/// actions that need one of them are blocked with that failure (D122, D126).
pub type FailedActions = BTreeMap<ActionId, Failure>;

/// A compensating action registered by a successful call (the api's `cleanup`), run when the
/// owning frame closes, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PendingCleanup {
    /// The cleanup action.
    pub action: ActionId,
    /// Its vars, rendered against the registering call's response; they stand in for its own.
    pub vars: Arc<Saved>,
}

impl PendingCleanup {
    /// A cleanup of `action` with no vars.
    #[must_use]
    pub fn new(action: ActionId) -> Self {
        Self { action, vars: Arc::default() }
    }

    /// The same cleanup with `vars`.
    #[must_use]
    pub fn with_vars(self, vars: Saved) -> Self {
        Self { vars: Arc::new(vars), ..self }
    }
}

/// What a worker reports when an action finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActionReport {
    /// How it ended.
    pub outcome: ActionOutcome,
    /// Cleanups it registered, oldest first.
    pub cleanups: Vec<PendingCleanup>,
    /// Values it saved.
    pub saved: Saved,
    /// Vars defined as one `${fake.…}` that it generated, kept for the rest of the instance.
    pub fakes: Saved,
    /// The response of its last call, for `${<id>.…}` in later actions.
    pub response: Option<Arc<serde_json::Value>>,
    /// Its evaluated checks, in order, each reported as a `CheckEvaluated` event.
    pub checks: Vec<CheckResult>,
    /// Its calls' exchanges, whole, each reported as an `Exchanged` event before the checks;
    /// the redactor masks them, and only history and reports cut the bodies afterwards.
    pub exchanges: Vec<Exchange>,
    /// Reads of values a `function:` or `test:` action saved without declaring them in
    /// `provides`, each a V5108 warning at the action that saved the value (D186), reported as a
    /// `Warned` event.
    pub warnings: Vec<Failure>,
    /// Which of `exchanges` its failure's part sent, when only part of it failed: a loop's
    /// failing turn or a reused test's failing step. Empty when that part sent nothing; `None`
    /// means all of them.
    pub failing: Option<Range<usize>>,
}

impl ActionReport {
    /// A passed action with no cleanups and no saved values.
    #[must_use]
    pub fn passed() -> Self {
        Self {
            outcome: ActionOutcome::Passed,
            cleanups: Vec::new(),
            saved: Saved::new(),
            fakes: Saved::new(),
            response: None,
            checks: Vec::new(),
            exchanges: Vec::new(),
            warnings: Vec::new(),
            failing: None,
        }
    }

    /// A failed action.
    #[must_use]
    pub fn failed(failure: Failure) -> Self {
        Self { outcome: ActionOutcome::Failed(failure), ..Self::passed() }
    }

    /// The positions its failure's part's exchanges take once they follow `before` others.
    #[must_use]
    pub fn failing_after(&self, before: usize) -> Range<usize> {
        let Range { start, end } = self.failing.clone().unwrap_or(0..self.exchanges.len());
        before + start..before + end
    }
}

/// The cleanup actions still on `stack`, newest first, for a `CleanupsAbandoned` event.
fn newest_first(stack: &[PendingCleanup]) -> Vec<ActionId> {
    stack.iter().rev().map(|cleanup| cleanup.action).collect()
}

/// The shared lifecycle actions of one frame (`beforeRun` or `beforeAll`) and which failed.
#[derive(Clone, Debug)]
struct SharedActions {
    owner: OwnerKey,
    failed: FailedActions,
}

impl SharedActions {
    const fn new(owner: OwnerKey) -> Self {
        Self { owner, failed: FailedActions::new() }
    }

    /// From `index`, the position and ID of the next action in `planned` to run, or `None` when
    /// they are all done. An action that needs a failed one is not run: it fails with that
    /// failure, reported in `events`.
    fn next(&mut self, planned: &[PlannedAction], index: usize, events: &mut Vec<Event>) -> (usize, Option<ActionId>) {
        let mut index = index;
        while let Some(entry) = planned.get(index) {
            let Some(failure) = self.blocking(&entry.needs) else { return (index, Some(entry.action)) };
            self.failed.insert(entry.action, failure.clone());
            let outcome = ActionOutcome::Failed(failure);
            events.push(Event::ActionFinished { action: entry.action, owner: self.owner, outcome });
            index += 1;
        }
        (index, None)
    }

    /// Notes the outcome of `entry`, the action that just ran.
    fn record(&mut self, entry: Option<&PlannedAction>, outcome: ActionOutcome) {
        if let (Some(entry), ActionOutcome::Failed(failure)) = (entry, outcome) {
            self.failed.insert(entry.action, failure);
        }
    }

    /// The failure of the first failed action among `needs`, which blocks the dependant.
    fn blocking(&self, needs: &Needs) -> Option<Failure> {
        needs.iter().find_map(|action| self.failed.get(&action)).cloned()
    }
}

#[cfg(test)]
mod props;
#[cfg(test)]
mod sim;
