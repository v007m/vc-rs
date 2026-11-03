//! Shared lifecycle values without locks: one write-once slot per shared action.

use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::OnceCell;
use vc_model::{ActionId, ActionOutcome, Failure};

use crate::{ActionReport, Saved};

/// What a shared action left behind: its saved values, or its failure.
pub type SharedResult = Result<Arc<Saved>, Arc<Failure>>;

/// The results of one frame's shared actions (`beforeRun` or a file's `beforeAll`): a slot per
/// planned action, created up front and filled once by the frame's owner task, then read by
/// every instance (D122). `OnceCell` makes each slot write-once, so no lock is needed (D113).
#[derive(Debug, Default)]
pub struct SharedSlots {
    slots: BTreeMap<ActionId, OnceCell<SharedResult>>,
}

impl SharedSlots {
    /// An empty slot for each of `actions`.
    #[must_use]
    pub fn new(actions: impl IntoIterator<Item = ActionId>) -> Self {
        Self { slots: actions.into_iter().map(|action| (action, OnceCell::new())).collect() }
    }

    /// Fills the slot of `action` from its report. Returns false when `action` has no slot or
    /// its slot is already filled; the first result stands.
    pub fn fill(&self, action: ActionId, report: &ActionReport) -> bool {
        let result = match &report.outcome {
            ActionOutcome::Passed => Ok(Arc::new(report.saved.clone())),
            ActionOutcome::Failed(failure) => Err(Arc::new(failure.clone())),
        };
        self.slots.get(&action).is_some_and(|slot| slot.set(result).is_ok())
    }

    /// The result of `action`, once its slot is filled.
    #[must_use]
    pub fn get(&self, action: ActionId) -> Option<SharedResult> {
        self.slots.get(&action)?.get().cloned()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_model::{ActionId, Failure};

    use super::SharedSlots;
    use crate::ActionReport;

    #[test]
    fn a_slot_is_filled_once_and_unplanned_actions_have_none() {
        let slots = SharedSlots::new([ActionId::new(1), ActionId::new(2)]);
        assert_eq!(slots.get(ActionId::new(1)), None);
        let mut saved = ActionReport::passed();
        saved.saved.insert("userId".into(), json!(7));
        assert!(slots.fill(ActionId::new(1), &saved));
        assert!(!slots.fill(ActionId::new(1), &ActionReport::passed()), "the first result stands");
        assert_eq!(slots.get(ActionId::new(1)).and_then(Result::ok).map(|saved| saved["userId"].clone()), Some(json!(7)));
        assert!(slots.fill(ActionId::new(2), &ActionReport::failed(Failure::new("down"))));
        assert_eq!(slots.get(ActionId::new(2)).and_then(Result::err).map(|failure| failure.message.clone()), Some("down".into()));
        assert!(!slots.fill(ActionId::new(3), &ActionReport::passed()));
        assert_eq!(slots.get(ActionId::new(3)), None);
    }
}
