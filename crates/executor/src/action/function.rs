//! `function:` actions: a hook export called through the [`HookRunner`] port (D172).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::HookRunner;
use vc_model::{Action, ActionId, ActionKind, Failure, Program};

use super::hooks::{self, HookBridge};
use super::{ActionExecutor, ActionRequest};
use super::{provides, request};
use crate::{ActionReport, Render, Saved};

/// Runs `function:` actions through a [`HookRunner`].
///
/// The function gets `{vars, run}`: every var in the action's scope that renders, and the run's
/// `profiles`. When it returns an object, each of its fields is saved under its name; anything
/// else saves nothing; a name the action declares in `provides` that is not among them is V5109.
/// A function that throws or overruns its budget fails the action (V5115,
/// V5116), as does any function when the run has no runner.
#[derive(Clone, Default)]
pub struct FunctionExecutor {
    runner: Option<Arc<dyn HookRunner>>,
}

impl std::fmt::Debug for FunctionExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionExecutor").field("runner", &self.runner.is_some()).finish()
    }
}

impl FunctionExecutor {
    /// An executor that calls each function with `runner`; with none, each fails with V5115.
    #[must_use]
    pub const fn new(runner: Option<Arc<dyn HookRunner>>) -> Self {
        Self { runner }
    }

    async fn function(&self, request: &ActionRequest) -> Result<ActionReport, Failure> {
        let action =
            request.cx.action(request.action).ok_or_else(|| Failure::new("the action is not in the program"))?;
        let ActionKind::Function(hook) = &action.kind else {
            return Err(Failure::new("the action calls no function"));
        };
        let render = Render::new(request).map_err(request::failed)?;
        let input = json!({ "vars": render.hook_vars(), "run": { "profiles": render.values().profiles() } });
        let fakes = render.into_memo();
        let bridge = HookBridge::new(self.runner.clone(), &request.cx.program);
        let returned = bridge.call(hook, input).await.map_err(|error| hooks::failure(hook, &error))?;
        let saved = saved(returned);
        if let Some(failure) = provides::unsaved(&request.cx.program, action, &saved) {
            return Err(failure);
        }
        Ok(ActionReport { saved, fakes, ..ActionReport::passed() })
    }
}

/// The fields of an object a function returned; nothing for any other value.
fn saved(returned: Value) -> Saved {
    match returned {
        Value::Object(fields) => fields,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => Saved::new(),
    }
}

#[async_trait]
impl ActionExecutor for FunctionExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        self.function(&request).await.unwrap_or_else(|failure| ActionReport::failed(failure.at(request.action)))
    }

    fn follows(&self, _program: &Program, _action: &Action) -> Vec<ActionId> {
        vec![]
    }
}
