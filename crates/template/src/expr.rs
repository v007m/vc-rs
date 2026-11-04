use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use vc_model::Namespace;

/// How many comprehension steps one evaluation may take unless the activation says otherwise.
pub const DEFAULT_STEP_LIMIT: u64 = 1_000_000;

/// A prepared expression, evaluated against a fresh [`Activation`] each time.
///
/// [`crate::CelCompiler::prepare`] implements it over the CEL interpreter, which stays private
/// to this crate so it can be replaced.
pub trait Expr: Send + Sync + fmt::Debug {
    /// Evaluates the expression.
    ///
    /// # Errors
    ///
    /// [`ExprError`] when evaluation fails, takes too many steps, or yields a value JSON cannot
    /// hold.
    fn eval(&self, activation: &Activation) -> Result<Value, ExprError>;
}

/// The values of the roots for one evaluation (D125, D150), and its step limit.
#[derive(Clone, Debug, Default)]
pub struct Activation {
    roots: HashMap<Namespace, Arc<Value>>,
    step_limit: Option<u64>,
}

impl Activation {
    /// No roots, and the [`DEFAULT_STEP_LIMIT`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the value of a root: `response` is `{status, headers, body, timing}`, `var` the vars
    /// in scope, a step id that step's response.
    #[must_use]
    pub fn with(mut self, root: Namespace, value: impl Into<Arc<Value>>) -> Self {
        self.roots.insert(root, value.into());
        self
    }

    /// Sets how many comprehension steps the evaluation may take.
    #[must_use]
    pub const fn with_step_limit(mut self, limit: u64) -> Self {
        self.step_limit = Some(limit);
        self
    }

    /// The value of a root, if set.
    #[must_use]
    pub fn get(&self, root: Namespace) -> Option<&Value> {
        self.roots.get(&root).map(AsRef::as_ref)
    }

    /// How many comprehension steps the evaluation may take.
    #[must_use]
    pub fn step_limit(&self) -> u64 {
        self.step_limit.unwrap_or(DEFAULT_STEP_LIMIT)
    }
}

/// Why evaluating an expression failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExprError {
    /// The expression failed: no such key, a division by zero, mismatched types…
    #[error("{0}")]
    Eval(String),
    /// The expression took more comprehension steps than the activation allows.
    #[error("the expression took more than {0} steps; loop over fewer elements")]
    StepLimit(u64),
    /// A root the expression reads has no value in the activation.
    #[error("`{0}` has no value in this evaluation")]
    MissingRoot(String),
    /// The result has no JSON form, such as bytes or a NaN.
    #[error("the result is {0}, which has no JSON form")]
    Unrepresentable(String),
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vc_model::Namespace;

    use super::{Activation, DEFAULT_STEP_LIMIT};

    #[test]
    fn activations_hold_roots_and_a_step_limit() {
        let activation = Activation::new().with(Namespace::Var, json!({"a": 1}));
        assert_eq!(activation.get(Namespace::Var), Some(&json!({"a": 1})));
        assert_eq!(activation.get(Namespace::Env), None);
        assert_eq!(activation.step_limit(), DEFAULT_STEP_LIMIT);
        assert_eq!(activation.with_step_limit(5).step_limit(), 5);
    }
}
