//! The values an action runs with, from the frame that runs it.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;
use vc_model::{Action, ActionId, ActionKind, FileId, Instance, Program, Sym};

use super::run::Defined;
use crate::{ActionReport, CleanupSink, Saved, SharedSlots};

/// What an action can read from its frame.
///
/// That is the shared lifecycle results, and in a test instance what its earlier actions left.
/// The instance's task owns its frame and absorbs each action's report into it; actions get a
/// copy.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Frame {
    /// `beforeRun` results.
    pub run: Arc<SharedSlots>,
    /// The file's `beforeAll` results; empty for run-level actions.
    pub file: Arc<SharedSlots>,
    /// The test instance, for its test's vars, its example row and its iteration.
    pub instance: Option<Arc<Instance>>,
    /// Values saved earlier in the instance; the latest wins.
    pub saved: Arc<Saved>,
    /// The saved values a `function:` or `test:` action left without declaring them in
    /// `provides`, by name, with that action; reading one is a V5108 warning (D186).
    pub origins: Arc<BTreeMap<String, ActionId>>,
    /// The last response of each earlier action with an `id`, by that id.
    pub steps: Arc<BTreeMap<Sym, Arc<Value>>>,
    /// Vars defined as one `${fake.…}`, generated once per instance (D131).
    pub fakes: Arc<Saved>,
    /// A cleanup's vars, rendered when its call registered it; set only while running a
    /// cleanup, which registers no cleanup of its own.
    pub given: Option<Arc<Saved>>,
    /// The vars of each `test:` reuse the action runs inside, outermost first.
    pub callers: Vec<Arc<Defined>>,
    /// Inside a `test:` reuse, the reused test's file, whose vars its steps read in place of the
    /// action owner's.
    pub test_file: Option<FileId>,
    /// Inside a `repeat` or `forEach` loop, the turn the action runs as.
    pub iteration: Option<Arc<Iteration>>,
    /// Where nested actions hand their cleanups as each finishes, so an action dropped past a
    /// `timeout` keeps those its finished parts registered.
    pub sink: Option<CleanupSink>,
    /// A load iteration with `checks: api`: a call evaluates only its api's own `expect`, not
    /// the test's or the step's. The default-2xx rule still holds: when that `expect` checks no
    /// status, the call is judged by `status: 2xx` first, as every call is.
    pub api_checks_only: bool,
}

/// One turn of a `repeat` or `forEach` loop.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Iteration {
    /// Which turn, from 1.
    pub index: usize,
    /// How many turns the loop has.
    pub count: usize,
    /// In a `forEach`, the element this turn runs with.
    pub item: Option<Value>,
}

impl Frame {
    /// Takes in what `action` left: its saves, its generated fakes and its response.
    pub fn absorb(&mut self, program: &Program, action: ActionId, report: &ActionReport) {
        Arc::make_mut(&mut self.saved).extend(report.saved.clone());
        Arc::make_mut(&mut self.fakes).extend(report.fakes.clone());
        let saver = program.actions.get(usize::try_from(action.index()).unwrap_or(usize::MAX));
        if let (Some(id), Some(response)) = (saver.and_then(|saver| saver.id), &report.response) {
            Arc::make_mut(&mut self.steps).insert(id, Arc::clone(response));
        }
        if !report.saved.is_empty() {
            self.trace(program, (action, saver), &report.saved);
        }
    }

    /// Notes which of `saved` its saving action (`step`) left undeclared; a name saved again by
    /// any other action is no longer undeclared.
    fn trace(&mut self, program: &Program, (action, step): (ActionId, Option<&Action>), saved: &Saved) {
        let origins = Arc::make_mut(&mut self.origins);
        for name in saved.keys() {
            if step.is_some_and(|step| undeclared(program, step, name)) {
                origins.insert(name.clone(), action);
            } else {
                origins.remove(name);
            }
        }
    }

    /// Forgets what the instance's earlier actions left, for a new attempt.
    pub fn restart(&mut self) {
        self.saved = Arc::default();
        self.origins = Arc::default();
        self.steps = Arc::default();
        self.fakes = Arc::default();
    }

    /// The frame a cleanup runs in: this one with the cleanup's vars.
    #[must_use]
    pub fn cleaning(&self, vars: &Arc<Saved>) -> Self {
        Self { given: Some(Arc::clone(vars)), ..self.clone() }
    }
}

/// Whether `saver` is a `function:` or `test:` action that saves `name` without declaring it in
/// `provides`.
fn undeclared(program: &Program, saver: &Action, name: &str) -> bool {
    let dynamic = matches!(saver.kind, ActionKind::Function(_) | ActionKind::Test(_));
    dynamic && !saver.provides.iter().any(|declared| program.str(declared.sym()) == Some(name))
}
