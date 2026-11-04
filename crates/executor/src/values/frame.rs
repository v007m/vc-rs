//! The values an action runs with, from the frame that runs it.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use vc_model::{ActionId, Instance, Program, Sym};

use super::run::Defined;
use crate::{ActionReport, Saved, SharedSlots};

/// What an action can read from its frame.
///
/// That is the shared lifecycle results, and in a test instance what its earlier actions left.
/// The instance's task owns its frame and absorbs each action's report into it; actions get a
/// copy.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    /// `beforeRun` results.
    pub run: Arc<SharedSlots>,
    /// The file's `beforeAll` results; empty for run-level actions.
    pub file: Arc<SharedSlots>,
    /// The test instance, for its test's vars, its example row and its iteration.
    pub instance: Option<Arc<Instance>>,
    /// Values saved earlier in the instance; the latest wins.
    pub saved: Arc<Saved>,
    /// The last response of each earlier action with an `id`, by that id.
    pub steps: Arc<BTreeMap<Sym, Arc<Value>>>,
    /// Vars defined as one `${fake.…}`, generated once per instance (D131).
    pub fakes: Arc<Saved>,
    /// A cleanup's vars, rendered when its call registered it; set only while running a
    /// cleanup, which registers no cleanup of its own.
    pub given: Option<Arc<Saved>>,
    /// The vars of each `test:` reuse the action runs inside, outermost first.
    pub callers: Vec<Arc<Defined>>,
}

impl Frame {
    /// Takes in what `action` left: its saves, its generated fakes and its response.
    pub fn absorb(&mut self, program: &Program, action: ActionId, report: &ActionReport) {
        Arc::make_mut(&mut self.saved).extend(report.saved.clone());
        Arc::make_mut(&mut self.fakes).extend(report.fakes.clone());
        let id =
            program.actions.get(usize::try_from(action.index()).unwrap_or(usize::MAX)).and_then(|action| action.id);
        if let (Some(id), Some(response)) = (id, &report.response) {
            Arc::make_mut(&mut self.steps).insert(id, Arc::clone(response));
        }
    }

    /// Forgets what the instance's earlier actions left, for a new attempt.
    pub fn restart(&mut self) {
        self.saved = Arc::default();
        self.steps = Arc::default();
        self.fakes = Arc::default();
    }

    /// The frame a cleanup runs in: this one with the cleanup's vars.
    #[must_use]
    pub fn cleaning(&self, vars: &Arc<Saved>) -> Self {
        Self { given: Some(Arc::clone(vars)), ..self.clone() }
    }
}
