//! Action executors (strategies), one per kind of action, chosen by the [`ActionExecutors`]
//! registry.

mod auth;
mod cached;
mod call;
mod command;
mod http_retry;
mod inline;
mod looping;
mod request;
mod reuse;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{AuthProvider, CacheStore};
use vc_model::{Action, ActionId, ActionKind, Failure, OwnerKey, Program};

use crate::{ActionReport, ExecutorError, Frame, MaskRules, PipelineParts, Redactor, RunInputs, RunValues};

pub use auth::{AuthKey, NoAuth, TokenCache};
pub use cached::CachedExecutor;
pub use call::CallExecutor;
pub use command::CommandExecutor;
pub use inline::InlineExecutor;
pub use reuse::{TestExecutor, body_actions};

pub use request::encode;

/// The kind of an action, as the registry keys its executors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct ActionRequest {
    /// The action to run.
    pub action: ActionId,
    /// The frame that owns its saves and cleanups.
    pub owner: OwnerKey,
    /// The program and the executors, for nested actions.
    pub cx: Arc<ActionCx>,
    /// What it can read from the frame that runs it.
    pub frame: Frame,
}

impl fmt::Debug for ActionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionRequest")
            .field("action", &self.action)
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

/// What every action of a run shares: the immutable program, the executor registry and the
/// run's values.
#[non_exhaustive]
pub struct ActionCx {
    /// The compiled program.
    pub program: Arc<Program>,
    /// The executors, by kind.
    pub executors: Arc<ActionExecutors>,
    /// The var definitions with the active profiles applied, the pipes and the run's inputs.
    pub values: RunValues,
    /// Masks what must not leave the executor, built from the config's `redact` and told every
    /// secret value.
    pub redactor: Arc<Redactor>,
    /// Obtains credentials for each auth type.
    auth: Arc<dyn AuthProvider>,
    /// The run's credentials, fetched once per auth and set of vars.
    tokens: Arc<TokenCache>,
    /// The cross-run cache, when the run has one.
    cache: Option<Arc<dyn CacheStore>>,
}

impl ActionCx {
    /// What the actions of a run of `program` share, given `inputs`.
    #[must_use]
    pub fn new(program: Arc<Program>, executors: Arc<ActionExecutors>, inputs: RunInputs) -> Self {
        let redactor = Redactor::for_config(&program.config);
        redactor.hide_values(inputs.secrets.iter().map(|(_, secret)| secret.expose()));
        let values = RunValues::new(&program, inputs);
        let (auth, tokens) = (Arc::new(NoAuth), Arc::new(TokenCache::default()));
        Self { program, executors, values, redactor: Arc::new(redactor), auth, tokens, cache: None }
    }

    /// The same context obtaining credentials through `provider` (the auth registry).
    #[must_use]
    pub fn with_auth(self, provider: Arc<dyn AuthProvider>) -> Self {
        Self { auth: provider, ..self }
    }

    /// The same context keeping `cache:`d results in `store` between runs.
    #[must_use]
    pub fn with_cache(self, store: Arc<dyn CacheStore>) -> Self {
        Self { cache: Some(store), ..self }
    }

    /// The same context making the config's `redact.rules` through `rules`: how a run adds a
    /// rule kind beyond `regex` and `cel` (a `function:` rule, M4). The secrets stay hidden.
    #[must_use]
    pub fn with_mask_rules(self, rules: &MaskRules) -> Self {
        let redactor = Arc::new(self.redactor.with_rules(rules.for_config(&self.program.config)));
        Self { redactor, ..self }
    }

    /// The action behind `id`.
    #[must_use]
    pub fn action(&self, id: ActionId) -> Option<&Action> {
        self.program.actions.get(usize::try_from(id.index()).ok()?)
    }
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

    /// `call:` and inline requests through the request pipeline built from `parts`, and `test:`
    /// reuse, both through the cross-run cache ([`CachedExecutor`]). `command:` needs a runner, so the engine adds a [`CommandExecutor`]; `function:`
    /// is registered in M4.
    #[must_use]
    pub fn standard(parts: PipelineParts) -> Self {
        Self::new()
            .with(ActionKindTag::Inline, Arc::new(InlineExecutor::new(parts.clone())))
            .with(ActionKindTag::Call, Arc::new(CachedExecutor::new(Arc::new(CallExecutor::new(parts)))))
            .with(ActionKindTag::Test, Arc::new(CachedExecutor::new(Arc::new(TestExecutor))))
    }

    /// The registry with `executor` running actions of `kind`.
    #[must_use]
    pub fn with(mut self, kind: ActionKindTag, executor: Arc<dyn ActionExecutor>) -> Self {
        self.by_kind.insert(kind, executor);
        self
    }

    /// Runs `request` with the executor for its kind, once per turn when the action has
    /// `repeat` or `forEach`; an action with no executor fails.
    pub async fn execute(&self, request: ActionRequest) -> ActionReport {
        let id = request.action;
        let action = request.cx.action(id);
        let (kind, spec) =
            (action.map(|action| ActionKindTag::of(&action.kind)), action.and_then(|a| a.repeat.clone()));
        match (kind.and_then(|kind| self.by_kind.get(&kind)), spec) {
            (Some(executor), Some(spec)) => looping::iterate(Arc::clone(executor), request, &spec).await,
            (Some(executor), None) => executor.execute(request).await,
            (None, _) => ActionReport::failed(Failure::new("no executor runs this action").at(id)),
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
