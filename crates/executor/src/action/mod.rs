//! Action executors (strategies), one per kind of action, chosen by the [`ActionExecutors`]
//! registry.

mod call;
mod reuse;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use vc_model::{Action, ActionId, ActionKind, Failure, OwnerKey, Program};

use crate::{ActionReport, ExecutorError, PipelineParts, SharedSlots};

pub use call::CallExecutor;
pub use reuse::TestExecutor;
pub use reuse::body_actions;

/// The kind of an action, as the registry keys its executors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ActionKindTag {
    /// `call:` an api.
    Call,
    /// `test:` reuse.
    Test,
    /// `function:` a hook export.
    Function,
    /// `command:` a shell command.
    Command,
    /// A request written in place (flow auth steps).
    Inline,
}

impl ActionKindTag {
    /// The tag of `kind`.
    #[must_use]
    pub const fn of(kind: &ActionKind) -> Self {
        match kind {
            ActionKind::Call(_) => Self::Call,
            ActionKind::Test(_) => Self::Test,
            ActionKind::Function(_) => Self::Function,
            ActionKind::Command(_) => Self::Command,
            ActionKind::Inline(_) => Self::Inline,
        }
    }

    /// The key it is written with.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Test => "test",
            Self::Function => "function",
            Self::Command => "command",
            Self::Inline => "inline",
        }
    }
}

impl fmt::Display for ActionKindTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Runs one kind of action.
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Runs `request.action` and reports how it ended, with the cleanups it registered.
    async fn execute(&self, request: ActionRequest) -> ActionReport;

    /// The further actions that running `action` can run (an api's cleanup, a reused test's
    /// steps), so the start-up check reaches them.
    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId>;
}

/// What an executor gets: the action, the frame it runs in, the program and the shared values.
#[derive(Clone)]
pub struct ActionRequest {
    /// The action to run.
    pub action: ActionId,
    /// The frame that owns its saves and cleanups.
    pub owner: OwnerKey,
    /// The program and the executors, for nested actions.
    pub cx: Arc<ActionCx>,
    /// The shared lifecycle values it can read.
    pub scope: Scope,
}

impl fmt::Debug for ActionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionRequest").field("action", &self.action).field("owner", &self.owner).finish_non_exhaustive()
    }
}

/// What every action of a run shares: the immutable program and the executor registry.
pub struct ActionCx {
    /// The compiled program.
    pub program: Arc<Program>,
    /// The executors, by kind.
    pub executors: Arc<ActionExecutors>,
}

impl ActionCx {
    /// The action behind `id`.
    #[must_use]
    pub fn action(&self, id: ActionId) -> Option<&Action> {
        self.program.actions.get(usize::try_from(id.index()).ok()?)
    }
}

/// The shared lifecycle values an action can read: `beforeRun` and its file's `beforeAll`.
#[derive(Clone, Debug, Default)]
pub struct Scope {
    /// `beforeRun` results.
    pub run: Arc<SharedSlots>,
    /// The file's `beforeAll` results; empty for run-level actions.
    pub file: Arc<SharedSlots>,
}

/// The action executors by kind (a registry filled at start-up).
#[derive(Clone, Default)]
pub struct ActionExecutors {
    by_kind: BTreeMap<ActionKindTag, Arc<dyn ActionExecutor>>,
}

impl fmt::Debug for ActionExecutors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_kind.keys()).finish()
    }
}

impl ActionExecutors {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `call:` through the request pipeline built from `parts`, and `test:` reuse. `function:`
    /// is registered in M4; `command:` when its runner lands.
    #[must_use]
    pub fn standard(parts: PipelineParts) -> Self {
        Self::new()
            .with(ActionKindTag::Call, Arc::new(CallExecutor::new(parts)))
            .with(ActionKindTag::Test, Arc::new(TestExecutor))
    }

    /// The registry with `executor` running actions of `kind`.
    #[must_use]
    pub fn with(mut self, kind: ActionKindTag, executor: Arc<dyn ActionExecutor>) -> Self {
        self.by_kind.insert(kind, executor);
        self
    }

    /// Runs `request` with the executor for its kind; an action with none fails.
    pub async fn execute(&self, request: ActionRequest) -> ActionReport {
        let id = request.action;
        let kind = request.cx.action(id).map(|action| ActionKindTag::of(&action.kind));
        match kind.and_then(|kind| self.by_kind.get(&kind)) {
            Some(executor) => executor.execute(request).await,
            None => ActionReport::failed(Failure::new("no executor runs this action").at(id)),
        }
    }

    /// Checks that every action reachable from `roots` exists and has an executor, so a run never
    /// starts what it cannot finish.
    ///
    /// # Errors
    ///
    /// [`ExecutorError::UnknownAction`] or [`ExecutorError::NoExecutor`] for the first one found.
    pub fn check(&self, program: &Program, roots: Vec<ActionId>) -> Result<(), ExecutorError> {
        let mut todo = roots;
        let mut seen = BTreeSet::new();
        while let Some(id) = todo.pop() {
            if !seen.insert(id) {
                continue;
            }
            let action = program.actions.get(usize::try_from(id.index()).unwrap_or(usize::MAX));
            let action = action.ok_or(ExecutorError::UnknownAction(id))?;
            let kind = ActionKindTag::of(&action.kind);
            let executor = self.by_kind.get(&kind).ok_or(ExecutorError::NoExecutor { action: id, kind })?;
            todo.extend(executor.follows(program, action));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
