//! The pure state machines that decide a run: [`RunOrchestrator`], [`FileOrchestrator`] and
//! [`InstanceWorkflow`]. Each is `transition(state, input) -> (state, commands)` with no I/O, no
//! clock and no async; the shells in `crate::shell` carry out the commands and feed back inputs.

mod file;
mod instance;
mod run;

use std::collections::BTreeMap;

use vc_model::{ActionId, ActionOutcome, Event, Failure, Needs, OwnerKey, PlannedAction};

pub use file::{FileCommand, FileInput, FileOrchestrator, FilePhase, Launch};
pub use instance::{InstanceCommand, InstanceInput, InstancePhase, InstanceScript, InstanceWorkflow, Verdict};
pub use run::{RunCommand, RunInput, RunOrchestrator, RunPhase, RunSummary};

/// Values an action saved, by name (filled by the vars runtime; empty until then).
pub type Saved = serde_json::Map<String, serde_json::Value>;

/// The shared lifecycle actions that failed, with their failures; instances and later shared
/// actions that need one of them are blocked with that failure (D122, D126).
pub type FailedActions = BTreeMap<ActionId, Failure>;

/// A compensating action registered by a successful call (the api's `cleanup`), run when the
/// owning frame closes, newest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingCleanup {
    /// The cleanup action.
    pub action: ActionId,
}

/// What a worker reports when an action finishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionReport {
    /// How it ended.
    pub outcome: ActionOutcome,
    /// Cleanups it registered, oldest first.
    pub cleanups: Vec<PendingCleanup>,
    /// Values it saved.
    pub saved: Saved,
}

impl ActionReport {
    /// A passed action with no cleanups and no saved values.
    #[must_use]
    pub fn passed() -> Self {
        Self { outcome: ActionOutcome::Passed, cleanups: Vec::new(), saved: Saved::new() }
    }

    /// A failed action.
    #[must_use]
    pub fn failed(failure: Failure) -> Self {
        Self { outcome: ActionOutcome::Failed(failure), ..Self::passed() }
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
