//! A `graphql` response's errors as `expect.errors` reads them (D345): each error normalised to
//! `{message, path, code, extensions}`, with `path` dotted as body paths are written and `code`
//! its `extensions.code`; and the internal matchers the written forms lower to: `fields` for an
//! expected error (D360) and `entries` for the list form, which pairs each entry with its own
//! error (D362). A report shows each as written.

use serde_json::{Map, Value, json};
use vc_model::{Matcher, MatcherArg, Segment};

use super::registry::Judged;
use super::{Judge, MatcherRegistry};
use crate::pairing::Pairing;

/// `registry` with `fields`, which holds when each field's matcher holds on that field, and
/// `entries`, which holds when the errors pair one to one with the entries.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry.internal("fields", fields).internal_reporting("entries", entries)
}

fn fields(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let MatcherArg::Fields(fields) = arg else { return Err("expected a matcher per field".to_owned()) };
    for (name, matcher) in fields {
        if !judge.matches(matcher, actual.and_then(|object| object.get(name)))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Holds when the errors pair one to one with the entries. A failure reports the errors with
/// their `count` when it is not the number of entries, or else with the entries, as written,
/// that no error was left for (`unpaired`).
fn entries(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<Judged, String> {
    let MatcherArg::Matchers(entries) = arg else { return Err("expected a list of expected errors".to_owned()) };
    let errors = actual.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    if errors.len() != entries.len() {
        return Ok(Judged::failed(json!({"count": errors.len(), "errors": errors})));
    }
    let unpaired = unpaired(judge, entries, errors)?;
    if unpaired.is_empty() {
        return Ok(Judged::from(true));
    }
    let written = unpaired.iter().map(|&entry| judge.expected(&entries[entry])).collect::<Result<Vec<_>, _>>()?;
    Ok(Judged::failed(json!({"errors": errors, "unpaired": written})))
}

/// The entries left without an error of their own once each is paired with a distinct error
/// it matches, as many as can be.
fn unpaired(judge: &Judge<'_>, entries: &[Matcher], errors: &[Value]) -> Result<Vec<usize>, String> {
    let fits = entries.iter().map(|entry| fits(judge, entry, errors)).collect::<Result<_, _>>()?;
    Ok(Pairing::new(fits, errors.len()).unpaired())
}

/// The indexes of the errors `entry` matches.
fn fits(judge: &Judge<'_>, entry: &Matcher, errors: &[Value]) -> Result<Vec<usize>, String> {
    let mut fits = Vec::new();
    for (index, error) in errors.iter().enumerate() {
        if judge.matches(entry, Some(error))? {
            fits.push(index);
        }
    }
    Ok(fits)
}

/// The errors of a response `body`, normalised; an empty list when it has none.
///
/// # Errors
///
/// What the check reports as the actual value when `body` is not a GraphQL response (not a
/// JSON object) or its `errors` is not a list.
pub fn normalised(body: Option<&Value>) -> Result<Vec<Value>, &'static str> {
    let Some(body) = body.and_then(Value::as_object) else { return Err("the body is not a GraphQL response") };
    match body.get("errors") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(errors)) => Ok(errors.iter().map(error).collect()),
        Some(_) => Err("`errors` is not a list"),
    }
}

/// One error with the fields it has: `message`, `path` dotted, `code` and `extensions`.
fn error(error: &Value) -> Value {
    let extensions = error.get("extensions");
    let path = error.get("path").and_then(Value::as_array).map(|path| Value::String(dotted(path)));
    let fields = [
        ("message", error.get("message").cloned()),
        ("path", path),
        ("code", extensions.and_then(|extensions| extensions.get("code")).cloned()),
        ("extensions", extensions.cloned()),
    ];
    let present = fields.into_iter().filter_map(|(name, value)| Some((name.to_owned(), value?)));
    Value::Object(present.collect::<Map<_, _>>())
}

/// `["users", 2, "email"]` as `users[2].email`; a segment that is neither a key nor a whole
/// number is a key of its JSON text.
fn dotted(path: &[Value]) -> String {
    let segment = |value: &Value| match value {
        Value::String(key) => Segment::Key(key.clone()),
        Value::Number(number) => number.as_i64().map_or_else(|| Segment::Key(number.to_string()), Segment::Index),
        other => Segment::Key(other.to_string()),
    };
    vc_template::dotted(&path.iter().map(segment).collect::<Vec<_>>())
}

#[cfg(test)]
#[path = "errors_tests.rs"]
mod tests;
