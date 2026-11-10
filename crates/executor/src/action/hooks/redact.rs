//! A `function:` redact rule (D171): a hook the redactor awaits, within the hook budget, for
//! each piece of output (D185).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::{HookRunner, Secrets};
use vc_model::{HookRef, Program, RedactRule};

use super::HookBridge;
use crate::{MASKED, MaskRule, MaskRuleMaker, MaskRules, RedactRuleKind};

/// What `function:` rules run on: the run's hook runner, if any, and the secrets its hooks can
/// read.
pub type RuleHooks = (Option<Arc<dyn HookRunner>>, Secrets);

/// A `function:` redact rule. The hook is given each piece of output whole, a text as
/// `{text}` and a JSON value as `{json}`, and returns it masked: a string for a text, any JSON
/// for a JSON value. A hook that throws, overruns its budget or returns a text that is not a
/// string masks the whole piece (fail closed).
struct HookMaskRule {
    bridge: HookBridge,
    hook: HookRef,
}

#[async_trait]
impl MaskRule for HookMaskRule {
    async fn text(&self, text: String) -> String {
        match self.bridge.call(&self.hook, json!({ "text": text })).await {
            Ok(Value::String(masked)) => masked,
            Ok(_) | Err(_) => MASKED.to_owned(),
        }
    }

    async fn value(&self, value: Value) -> Value {
        self.bridge.call(&self.hook, json!({ "json": value })).await.unwrap_or_else(|_| json!(MASKED))
    }
}

impl MaskRules {
    /// The registry with a maker for `function:` rules, which call their hooks through
    /// `runner` with `program`'s hook files, each able to read the run's `secrets` through
    /// `secret()`. What a hook returns is masked again by the built-in masking, so it cannot
    /// put a secret back. Without a runner such a rule masks everything.
    #[must_use]
    pub fn with_hooks(self, (runner, secrets): RuleHooks, program: &Program) -> Self {
        let bridge = HookBridge::new(runner, program).with_secrets(secrets);
        let maker: MaskRuleMaker = Arc::new(move |rule| match rule {
            RedactRule::Function(hook) => {
                let rule = HookMaskRule { bridge: bridge.clone(), hook: hook.clone() };
                Some(Arc::new(rule) as Arc<dyn MaskRule>)
            }
            RedactRule::Regex(_) | RedactRule::Cel(_) => None,
        });
        self.with(RedactRuleKind::Function, maker)
    }
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
