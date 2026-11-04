//! Shape matchers: `type`, `exists`, `empty`, `length`, `format`.

use regex::Regex;
use serde_json::Value;
use vc_model::MatcherArg;

use super::{Judge, MatcherRegistry, compare};

/// Whether a value is of one `type`.
type IsType = fn(&Value) -> bool;

/// Each `type` and the test for it.
const TYPES: [(&str, IsType); 7] = [
    ("string", Value::is_string),
    ("number", Value::is_number),
    ("integer", is_integer),
    ("boolean", Value::is_boolean),
    ("object", Value::is_object),
    ("array", Value::is_array),
    ("null", Value::is_null),
];

/// Each `format` and the pattern a string in it matches.
const FORMATS: [(&str, &str); 6] = [
    ("email", r"^[^@\s]+@[^@\s]+\.[^@\s]+$"),
    ("uuid", r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"),
    ("uri", r"^[A-Za-z][A-Za-z0-9+.\-]*:\S+$"),
    ("date", r"^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$"),
    (
        "dateTime",
        r"^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])[Tt ]([01]\d|2[0-3]):[0-5]\d:([0-5]\d|60)(\.\d+)?([Zz]|[+-]([01]\d|2[0-3]):[0-5]\d)$",
    ),
    ("ipv4", r"^((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)$"),
];

/// `registry` with the shape matchers.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry
        .with("type", type_of)
        .with("exists", exists)
        .with("empty", empty)
        .with("length", length)
        .with("format", format)
}

/// The length of a string (in characters), a list or an object.
fn length_of(value: &Value) -> Option<usize> {
    match value {
        Value::String(text) => Some(text.chars().count()),
        Value::Array(items) => Some(items.len()),
        Value::Object(fields) => Some(fields.len()),
        _ => None,
    }
}

fn is_integer(value: &Value) -> bool {
    value.as_f64().is_some_and(|number| number.fract() == 0.0)
}

fn flag(judge: &Judge<'_>, arg: &MatcherArg) -> Result<bool, String> {
    judge.value(arg)?.as_bool().ok_or_else(|| "expected `true` or `false`".to_owned())
}

fn named<T: Copy>(table: &[(&str, T)], name: &str) -> Result<T, String> {
    table.iter().find(|(known, _)| *known == name).map(|(_, entry)| *entry).ok_or_else(|| format!("unknown `{name}`"))
}

fn type_of(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let test = named(&TYPES, &judge.text(arg)?)?;
    Ok(actual.is_some_and(test))
}

fn exists(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    Ok(actual.is_some() == flag(judge, arg)?)
}

fn empty(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let wanted = flag(judge, arg)?;
    Ok(actual.and_then(length_of).is_some_and(|length| (length == 0) == wanted))
}

fn length(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let Some(length) = actual.and_then(length_of).map(Value::from) else { return Ok(false) };
    match arg {
        MatcherArg::Matcher(matcher) => judge.matches(matcher, Some(&length)),
        _ => Ok(compare::same(&length, &judge.value(arg)?)),
    }
}

fn format(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let pattern = Regex::new(named(&FORMATS, &judge.text(arg)?)?).map_err(|error| error.to_string())?;
    Ok(actual.and_then(Value::as_str).is_some_and(|text| pattern.is_match(text)))
}
