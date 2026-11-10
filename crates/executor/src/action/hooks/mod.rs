//! The bridge to the [`HookRunner`] port (D172): an api's `beforeRequest` and `afterResponse`
//! hooks, custom checks, `function:` actions and `function:` redact rules all call hooks through
//! a [`HookBridge`].

mod custom;
mod redact;
mod stage;

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use vc_diagnostics::Code;
use vc_interfaces::{Budget, HookCall, HookError, HookRunner};
use vc_model::{Api, Failure, HookModules, HookRef, Program, RequestHooks};

pub use custom::judge;
pub use stage::HookStage;

/// Calls hooks with the program's hook files through the run's runner, when it has one.
#[derive(Clone)]
pub struct HookBridge {
    runner: Option<Arc<dyn HookRunner>>,
    modules: HookModules,
    /// The time each call may run, when not the default budget's.
    time: Option<Duration>,
}

impl HookBridge {
    /// A bridge to `runner` with `program`'s hook files.
    pub fn new(runner: Option<Arc<dyn HookRunner>>, program: &Program) -> Self {
        Self { runner, modules: program.hooks.clone(), time: None }
    }

    /// This bridge, giving each call `time` to run when it is set (a `function:` action's
    /// `timeout`, D192); the memory budget stays the default.
    #[must_use]
    pub fn within(self, time: Option<Duration>) -> Self {
        Self { time, ..self }
    }

    /// What `hook` returns for `input`.
    ///
    /// # Errors
    ///
    /// The runner's [`HookError`]; [`HookError::Unavailable`] when the run has no runner.
    pub async fn call(&self, hook: &HookRef, input: Value) -> Result<Value, HookError> {
        let Some(runner) = &self.runner else {
            return Err(HookError::Unavailable { reason: "this run has no hook runner".to_owned() });
        };
        let mut call = HookCall::new(self.modules.clone(), hook.clone(), input);
        if let Some(time) = self.time {
            let mut budget = Budget::default();
            budget.time = time;
            call = call.with_budget(budget);
        }
        runner.call(call).await.map(|output| output.value)
    }
}

/// The failure of `hook`, under its code: V5116 for an overrun, V5115 for anything else.
pub fn failure(hook: &HookRef, error: &HookError) -> Failure {
    coded(hook, &error.to_string(), error.code())
}

/// The V5115 failure of a hook that returned what its place does not take.
pub fn returned(hook: &HookRef, what: &str) -> Failure {
    coded(hook, what, Code::V5115)
}

fn coded(hook: &HookRef, message: &str, code: Code) -> Failure {
    Failure::new(format!("`{}#{}`: {message}", hook.file, hook.export)).with_code(code.to_string())
}

/// `api`'s request hooks: its api file's, then its own.
pub fn request_hooks(program: &Program, api: &Api) -> RequestHooks {
    let group = program.api_groups.get(&api.group).map(|group| &group.hooks);
    let mut hooks = RequestHooks::default();
    for level in group.into_iter().chain([&api.hooks]) {
        hooks.before_request.extend(level.before_request.iter().cloned());
        hooks.after_response.extend(level.after_response.iter().cloned());
    }
    hooks
}

/// The JSON kind of `value`, for a message.
pub const fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
#[path = "hooks_tests.rs"]
mod tests;
