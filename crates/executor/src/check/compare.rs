//! Comparison matchers: `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `between`, `in`, `notIn`.

use std::cmp::Ordering;

use serde_json::Value;
use vc_model::MatcherArg;

use super::{Judge, MatcherRegistry};

/// `registry` with the comparison matchers.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry
        .with("eq", eq)
        .with("ne", ne)
        .with("gt", |judge, actual, arg| ordered(judge, actual, (arg, Ordering::is_gt)))
        .with("gte", |judge, actual, arg| ordered(judge, actual, (arg, Ordering::is_ge)))
        .with("lt", |judge, actual, arg| ordered(judge, actual, (arg, Ordering::is_lt)))
        .with("lte", |judge, actual, arg| ordered(judge, actual, (arg, Ordering::is_le)))
        .with("between", between)
        .with("in", one_of)
        .with("notIn", |judge, actual, arg| one_of(judge, actual, arg).map(|found| !found))
}

/// Deep equality, with numbers equal by value: `1` equals `1.0`.
pub fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(_), Value::Number(_)) => order(left, right) == Some(Ordering::Equal),
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(left, right)| same(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left)| right.get(key).is_some_and(|right| same(left, right)))
        }
        _ => left == right,
    }
}

/// How two numbers or two strings order; `None` for anything else.
fn order(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64()?.partial_cmp(&right.as_f64()?),
        (Value::String(left), Value::String(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

fn eq(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let expected = judge.value(arg)?;
    Ok(actual.is_some_and(|actual| same(actual, &expected)))
}

fn ne(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    eq(judge, actual, arg).map(|equal| !equal)
}

/// Whether an ordering is the one a matcher wants.
type Wanted = fn(Ordering) -> bool;

fn ordered(judge: &Judge<'_>, actual: Option<&Value>, (arg, wanted): (&MatcherArg, Wanted)) -> Result<bool, String> {
    let bound = judge.value(arg)?;
    Ok(actual.and_then(|actual| order(actual, &bound)).is_some_and(wanted))
}

fn between(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let MatcherArg::Range(range) = arg else { return Err("`between` takes `[low, high]`".to_owned()) };
    let [low, high] = range.as_ref();
    let (low, high) = (judge.value(low)?, judge.value(high)?);
    let within = |actual: &Value| {
        order(actual, &low).is_some_and(Ordering::is_ge) && order(actual, &high).is_some_and(Ordering::is_le)
    };
    Ok(actual.is_some_and(within))
}

fn one_of(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let Value::Array(items) = judge.value(arg)? else { return Err("`in` and `notIn` take a list".to_owned()) };
    Ok(actual.is_some_and(|actual| items.iter().any(|item| same(actual, item))))
}
