//! CEL's `timestamp()` and `duration()` (D153), built without the `chrono` feature: the values
//! compare, add and subtract as CEL specifies, and `string()` writes them back.

use std::any::Any;
use std::cmp::Ordering;

use cel::common::traits::{Adder, Comparer, Subtractor};
use cel::common::types::{CelString, DURATION_TYPE, STRING_TYPE, TIMESTAMP_TYPE, Type};
use cel::common::value::{CowVal, StaticVal, Val};
use cel::{DeclarationError, Env, ExecutionError};
use serde_json::Value as Json;

use crate::time::{self, Duration, Timestamp};

/// The function that reads a timestamp.
pub const TIMESTAMP: &str = "timestamp";

/// The function that reads a duration.
pub const DURATION: &str = "duration";

/// A CEL `google.protobuf.Timestamp`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CelTime(pub Timestamp);

/// A CEL `google.protobuf.Duration`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CelSpan(pub Duration);

/// Adds `timestamp(string)`, `duration(string)` and `string()` of each to `env`.
///
/// # Errors
///
/// When `env` already declares one of them.
pub fn register(env: &mut Env) -> Result<(), DeclarationError> {
    let string = || vec![STRING_TYPE.clone()];
    env.add_overload(TIMESTAMP, "timestamp(string)", string(), read_timestamp)?;
    env.add_overload(DURATION, "duration(string)", string(), read_duration)?;
    env.add_overload("string", "string(google.protobuf.Timestamp)", vec![TIMESTAMP_TYPE.clone()], write)?;
    env.add_overload("string", "string(google.protobuf.Duration)", vec![DURATION_TYPE.clone()], write)
}

/// The JSON form of a timestamp or a duration: its `string()`.
pub fn to_json(value: &dyn Val) -> Option<Json> {
    text(value).map(Json::String)
}

fn text(value: &dyn Val) -> Option<String> {
    let time = value.downcast_ref::<CelTime>().map(|time| time.0.to_string());
    time.or_else(|| value.downcast_ref::<CelSpan>().map(|span| span.0.to_string()))
}

fn argument<'a>(args: &'a [CowVal<'_, '_>], function: &str) -> Result<&'a str, ExecutionError> {
    args.first()
        .and_then(|arg| arg.downcast_ref::<CelString<'_>>())
        .map(CelString::inner)
        .ok_or_else(|| ExecutionError::function_error(function, format!("{function}() takes a string")))
}

fn read_timestamp<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let text = argument(&args, TIMESTAMP)?;
    let time = time::parse_timestamp(text).map_err(|error| ExecutionError::function_error(TIMESTAMP, error))?;
    Ok(CowVal::owned(CelTime(time)))
}

fn read_duration<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let text = argument(&args, DURATION)?;
    let span = time::parse_duration(text).map_err(|error| ExecutionError::function_error(DURATION, error))?;
    Ok(CowVal::owned(CelSpan(span)))
}

fn write<'b, 'v>(args: Vec<CowVal<'b, 'v>>) -> Result<CowVal<'b, 'v>, ExecutionError> {
    let shown = args.first().and_then(|arg| text(arg.as_ref()));
    let shown = shown.ok_or_else(|| ExecutionError::function_error("string", "string() of an unknown value"))?;
    Ok(CowVal::owned(CelString::from(shown)))
}

fn mismatch(operator: &str, left: &dyn Val, right: &dyn Val) -> ExecutionError {
    let message = format!("no `{operator}` for {} and {}", left.get_type().name(), right.get_type().name());
    ExecutionError::function_error(operator, message)
}

fn overflow(operator: &str) -> ExecutionError {
    ExecutionError::function_error(operator, "the result is out of range")
}

/// `Val` for a time value: it adds, subtracts, compares and is equal to its own kind.
macro_rules! time_val {
    ($name:ident, $kind:ident) => {
        impl Val for $name {
            fn get_type(&self) -> &Type {
                &$kind
            }

            fn cel_type() -> &'static Type {
                &$kind
            }

            fn as_adder<'b, 'v>(&'b self) -> Option<&'b (dyn Adder + 'v)>
            where
                Self: 'v,
            {
                Some(self)
            }

            fn as_comparer(&self) -> Option<&dyn Comparer> {
                Some(self)
            }

            fn as_subtractor<'b, 'v>(&'b self) -> Option<&'b (dyn Subtractor + 'v)>
            where
                Self: 'v,
            {
                Some(self)
            }

            fn equals(&self, other: &dyn Val) -> bool {
                other.downcast_ref::<Self>() == Some(self)
            }

            fn clone_as_boxed<'v>(&self) -> Box<dyn Val + 'v>
            where
                Self: 'v,
            {
                Box::new(*self)
            }

            fn as_any(&self) -> Option<&dyn Any> {
                Some(self)
            }
        }

        impl StaticVal for $name {}

        impl Comparer for $name {
            fn compare(&self, rhs: &dyn Val) -> Result<Ordering, ExecutionError> {
                let other = rhs.downcast_ref::<Self>().ok_or_else(|| mismatch("<", self, rhs))?;
                Ok(self.0.cmp(&other.0))
            }
        }
    };
}

time_val!(CelTime, TIMESTAMP_TYPE);
time_val!(CelSpan, DURATION_TYPE);

impl Adder for CelTime {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        let span = rhs.downcast_ref::<CelSpan>().ok_or_else(|| mismatch("+", self, rhs))?;
        self.0.plus(span.0).map(|time| CowVal::owned(Self(time))).ok_or_else(|| overflow("+"))
    }
}

impl Subtractor for CelTime {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(span) = rhs.downcast_ref::<CelSpan>() {
            return self.0.plus(span.0.negated()).map(|time| CowVal::owned(Self(time))).ok_or_else(|| overflow("-"));
        }
        let time = rhs.downcast_ref::<Self>().ok_or_else(|| mismatch("-", self, rhs))?;
        self.0.since(time.0).map(|span| CowVal::owned(CelSpan(span))).ok_or_else(|| overflow("-"))
    }
}

impl Adder for CelSpan {
    fn add<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        if let Some(time) = rhs.downcast_ref::<CelTime>() {
            return time.0.plus(self.0).map(|time| CowVal::owned(CelTime(time))).ok_or_else(|| overflow("+"));
        }
        let span = rhs.downcast_ref::<Self>().ok_or_else(|| mismatch("+", self, rhs))?;
        self.0.plus(span.0).map(|span| CowVal::owned(Self(span))).ok_or_else(|| overflow("+"))
    }
}

impl Subtractor for CelSpan {
    fn sub<'b, 'v>(&'b self, rhs: &(dyn Val + 'v)) -> Result<CowVal<'b, 'v>, ExecutionError>
    where
        Self: 'v,
    {
        let span = rhs.downcast_ref::<Self>().ok_or_else(|| mismatch("-", self, rhs))?;
        self.0.plus(span.0.negated()).map(|span| CowVal::owned(Self(span))).ok_or_else(|| overflow("-"))
    }
}
