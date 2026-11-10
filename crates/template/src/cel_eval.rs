//! Evaluates a prepared CEL program against an [`Activation`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cel::common::ast::{CallExpr, EntryExpr, Expr as Node, IdedExpr};
use cel::extractors::This;
use cel::objects::Key;
use cel::{Context, Env, ExecutionError, Value};
use serde_json::Value as Json;
use vc_model::Namespace;

use crate::cel_lex::VAR_ALIAS;
use crate::cel_time;
use crate::expr::{Activation, Expr, ExprError};

/// The function every comprehension's range is passed through, to count steps. The `@` keeps
/// it out of reach of expressions, as for the macros' `@result`.
const STEPS: &str = "@steps";

/// `sum()`, added to the standard library.
const SUM: &str = "sum";

/// A parsed CEL program, passed through [`instrument`], and the roots it reads.
pub struct CelExpr {
    /// The standard library.
    pub env: Arc<Env>,
    /// The instrumented program.
    pub expr: IdedExpr,
    /// For each root the program reads, its name in the program and the activation root.
    pub bindings: Vec<(String, Namespace)>,
    /// The expression as written, for debugging.
    pub text: String,
}

impl CelExpr {
    fn context(&self, activation: &Activation) -> Result<Context<'_, '_>, ExprError> {
        let mut context = Context::with_env(Arc::clone(&self.env));
        for (name, root) in &self.bindings {
            let shown = if name == VAR_ALIAS { "var" } else { name.as_str() };
            let value = activation.get(*root).ok_or_else(|| ExprError::MissingRoot(shown.to_owned()))?;
            context.add_variable_from_value(name.as_str(), from_json(value));
        }
        let used = AtomicU64::new(0);
        let limit = activation.step_limit();
        let steps = move |This(range): This<Value>| take_steps(range, &used, limit);
        context.add_function(STEPS, steps).map_err(|error| ExprError::Eval(error.to_string()))?;
        context.add_function(SUM, sum).map_err(|error| ExprError::Eval(error.to_string()))?;
        Ok(context)
    }
}

impl Expr for CelExpr {
    fn eval(&self, activation: &Activation) -> Result<Json, ExprError> {
        let context = self.context(activation)?;
        let failed = |error| match error {
            ExecutionError::FunctionError { function, .. } if function == STEPS => {
                ExprError::StepLimit(activation.step_limit())
            }
            other => ExprError::Eval(other.to_string()),
        };
        let value = Value::resolve_val(&self.expr, &context).map_err(failed)?;
        if let Some(time) = cel_time::to_json(value.as_ref()) {
            return Ok(time);
        }
        to_json(&Value::try_from(value.as_ref()).map_err(failed)?)
    }
}

impl fmt::Debug for CelExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CelExpr").field("text", &self.text).field("bindings", &self.bindings).finish_non_exhaustive()
    }
}

/// Passes every comprehension's range through [`STEPS`]: CEL has no step limit of its own, and
/// comprehensions are its only loops.
pub fn instrument(expr: &mut IdedExpr) {
    match &mut expr.expr {
        Node::Comprehension(comprehension) => {
            let range = std::mem::take(&mut comprehension.iter_range);
            let call = CallExpr { func_name: STEPS.to_owned(), target: None, args: vec![range] };
            comprehension.iter_range = IdedExpr { id: expr.id, expr: Node::Call(call) };
            let parts = [
                &mut comprehension.iter_range,
                &mut comprehension.accu_init,
                &mut comprehension.loop_cond,
                &mut comprehension.loop_step,
                &mut comprehension.result,
            ];
            parts.into_iter().for_each(instrument);
        }
        Node::Call(call) => call.target.iter_mut().map(AsMut::as_mut).chain(&mut call.args).for_each(instrument),
        Node::Select(select) => instrument(&mut select.operand),
        Node::List(list) => list.elements.iter_mut().for_each(instrument),
        Node::Map(map) => map.entries.iter_mut().for_each(|entry| instrument_entry(&mut entry.expr)),
        Node::Struct(object) => object.entries.iter_mut().for_each(|entry| instrument_entry(&mut entry.expr)),
        Node::Ident(_) | Node::Literal(_) | Node::Unspecified => {}
    }
}

fn instrument_entry(entry: &mut EntryExpr) {
    match entry {
        EntryExpr::MapEntry(pair) => {
            instrument(&mut pair.key);
            instrument(&mut pair.value);
        }
        EntryExpr::StructField(field) => instrument(&mut field.value),
    }
}

/// Counts the elements of a comprehension's range against the limit, and returns the range.
fn take_steps(range: Value, used: &AtomicU64, limit: u64) -> Result<Value, ExecutionError> {
    let size = match &range {
        Value::List(items) => items.len(),
        Value::Map(map) => map.map.len(),
        _ => 0,
    };
    let size = u64::try_from(size).unwrap_or(u64::MAX);
    let total = used.fetch_add(size, Ordering::Relaxed).saturating_add(size);
    if total > limit {
        return Err(ExecutionError::function_error(STEPS, format!("more than {limit} steps")));
    }
    Ok(range)
}

/// `list.sum()`: an `int` when every element is an `int`, otherwise a `double`; 0 when empty.
fn sum(This(list): This<Value>) -> Result<Value, ExecutionError> {
    let Value::List(items) = list else {
        return Err(ExecutionError::function_error(SUM, "sum() takes a list of numbers"));
    };
    items.iter().try_fold(Value::Int(0), |total, item| match (total, item) {
        (Value::Int(a), Value::Int(b)) => {
            a.checked_add(*b).map(Value::Int).ok_or_else(|| ExecutionError::function_error(SUM, "integer overflow"))
        }
        (Value::Int(a), Value::Float(b)) => Ok(Value::Float(as_double(a) + b)),
        (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + as_double(*b))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (_, other) => {
            let message = format!("sum() takes a list of numbers, not of {}", other.type_of());
            Err(ExecutionError::function_error(SUM, message))
        }
    })
}

#[expect(clippy::cast_precision_loss, reason = "CEL's double(int) converts the same way")]
const fn as_double(n: i64) -> f64 {
    n as f64
}

/// The CEL form of a JSON value. Every integer that fits is an `int`, so `var.price * 2` works:
/// CEL has no arithmetic across `int`, `uint` and `double`.
fn from_json(value: &Json) -> Value {
    match value {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_u64().map(Value::UInt))
            .unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN))),
        Json::String(s) => Value::from(s.as_str()),
        Json::Array(items) => Value::from(items.iter().map(from_json).collect::<Vec<_>>()),
        Json::Object(map) => {
            Value::from(map.iter().map(|(key, value)| (key.clone(), from_json(value))).collect::<HashMap<_, _>>())
        }
    }
}

/// The JSON form of a CEL value; map keys become strings, sorted.
fn to_json(value: &Value) -> Result<Json, ExprError> {
    match value {
        Value::Null => Ok(Json::Null),
        Value::Bool(b) => Ok(Json::Bool(*b)),
        Value::Int(n) => Ok(Json::from(*n)),
        Value::UInt(n) => Ok(Json::from(*n)),
        Value::Float(n) => serde_json::Number::from_f64(*n)
            .map(Json::Number)
            .ok_or_else(|| ExprError::Unrepresentable(format!("the number {n}"))),
        Value::String(s) => Ok(Json::String(s.as_str().to_owned())),
        Value::List(items) => items.iter().map(to_json).collect::<Result<_, _>>().map(Json::Array),
        Value::Map(map) => {
            let mut entries = map
                .map
                .iter()
                .map(|(key, value)| Ok((key_string(key), to_json(value)?)))
                .collect::<Result<Vec<_>, _>>()?;
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            Ok(Json::Object(entries.into_iter().collect()))
        }
        other => Err(ExprError::Unrepresentable(format!("a {} value", other.type_of()))),
    }
}

fn key_string(key: &Key) -> String {
    match key {
        Key::Int(n) => n.to_string(),
        Key::Uint(n) => n.to_string(),
        Key::Bool(b) => b.to_string(),
        Key::String(s) => s.as_str().to_owned(),
    }
}
