//! A tool call's arguments, read by name with the type its schema gives.

use serde_json::{Map, Value};

use crate::McpError;

/// The arguments of one tool call: a JSON object.
pub type Args = Map<String, Value>;

/// The string argument `name`.
pub fn text(args: &Args, name: &'static str) -> Result<String, McpError> {
    optional_text(args, name)?.ok_or_else(|| wrong(name, "is required"))
}

/// The string argument `name`, when given.
pub fn optional_text(args: &Args, name: &'static str) -> Result<Option<String>, McpError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(wrong(name, "must be a string")),
    }
}

/// The list-of-strings argument `name`; empty when not given.
pub fn texts(args: &Args, name: &'static str) -> Result<Vec<String>, McpError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => {
            items.iter().map(|item| item.as_str().map(str::to_owned).ok_or_else(|| strings(name))).collect()
        }
        Some(_) => Err(strings(name)),
    }
}

/// The boolean argument `name`; false when not given.
pub fn flag(args: &Args, name: &'static str) -> Result<bool, McpError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(flag)) => Ok(*flag),
        Some(_) => Err(wrong(name, "must be true or false")),
    }
}

/// The whole-number argument `name`, when given.
pub fn count(args: &Args, name: &'static str) -> Result<Option<usize>, McpError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .map(Some)
            .ok_or_else(|| wrong(name, "must be a whole number")),
    }
}

/// The object argument `name` as `key=value` pairs; empty when not given. A string value is
/// used as it is, any other value as its JSON.
pub fn pairs(args: &Args, name: &'static str) -> Result<Vec<String>, McpError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Object(map)) => Ok(map.iter().map(|(key, value)| format!("{key}={}", plain(value))).collect()),
        Some(_) => Err(wrong(name, "must be an object of names to values")),
    }
}

fn plain(value: &Value) -> String {
    value.as_str().map_or_else(|| value.to_string(), str::to_owned)
}

fn strings(name: &'static str) -> McpError {
    wrong(name, "must be a list of strings")
}

fn wrong(name: &'static str, reason: &str) -> McpError {
    McpError::Argument { name, reason: reason.to_owned() }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
