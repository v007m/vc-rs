use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{HookCall, HookError, HookOutput, HookRunner};

/// What a [`FakeHooks`] answers for one hook.
pub type Script = dyn Fn(&HookCall) -> Result<HookOutput, HookError> + Send + Sync;

/// A [`HookRunner`] that answers each `file#export` from a script; runs no JavaScript.
#[derive(Clone, Default)]
pub struct FakeHooks {
    scripts: BTreeMap<String, Arc<Script>>,
}

impl FakeHooks {
    /// A runner that knows no hooks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// This runner, answering `hook` (`file#export`) with `script`.
    #[must_use]
    pub fn on(
        mut self,
        hook: &str,
        script: impl Fn(&HookCall) -> Result<HookOutput, HookError> + Send + Sync + 'static,
    ) -> Self {
        self.scripts.insert(hook.to_owned(), Arc::new(script));
        self
    }
}

impl fmt::Debug for FakeHooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.scripts.keys()).finish()
    }
}

#[async_trait]
impl HookRunner for FakeHooks {
    async fn call(&self, call: HookCall) -> Result<HookOutput, HookError> {
        let hook = format!("{}#{}", call.hook.file, call.hook.export);
        self.scripts.get(&hook).map_or_else(|| Err(HookError::NotFound { hook }), |script| script(&call))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vc_interfaces::{HookError, HookOutput, HookRunner, Limit};

    use super::FakeHooks;
    use crate::{SUITE_HOOKS, hook_runner_suite};

    fn suite_fake() -> FakeHooks {
        let hook = |export: &str| format!("{SUITE_HOOKS}#{export}");
        let overran = |limit| move |_: &_| Err(HookError::Overran { limit, line: None });
        FakeHooks::new()
            .on(&hook("echo"), |call| Ok(HookOutput::new(call.input.clone())))
            .on(&hook("later"), |call| {
                Ok(HookOutput::new(json!({"doubled": call.input["n"].as_i64().unwrap_or(0) * 2})))
            })
            .on(&hook("boom"), |_| Err(HookError::Threw { message: "Error: bad input".into(), line: Some(14) }))
            .on(&hook("spin"), overran(Limit::Time))
            .on(&hook("hog"), overran(Limit::Memory))
            .on(&hook("key"), |call| {
                let value = call.secrets.get("KEY").map_or(Value::Null, |key| json!(key.expose()));
                Ok(HookOutput::new(value).reading(vec!["KEY".into()]))
            })
            .on(&hook("callable"), |_| Err(HookError::BadValue { what: "a function".into() }))
    }

    #[tokio::test]
    async fn fake_hooks_meet_the_contract() {
        hook_runner_suite(&mut || Box::new(suite_fake()) as Box<dyn HookRunner>).await;
        assert!(format!("{:?}", suite_fake()).contains("hooks/suite.ts#echo"));
    }
}
