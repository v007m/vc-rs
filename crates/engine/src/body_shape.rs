//! The JSON shape of each api's responses in two runs, and where it changed (D672): the paths
//! a body has and the JSON types at each, never the values.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use vc_interfaces::ExchangeRow;
use vc_model::one_line;

/// The segment that stands for any element of an array.
const ELEMENT: &str = "*";

/// The JSON types seen at each path, as JSON Pointers with [`ELEMENT`] for an array's elements.
type Shape = BTreeMap<String, BTreeSet<&'static str>>;

/// The responses of one api with one status, by `(api, status)`.
type Shapes = BTreeMap<Answer, Shape>;

/// A path whose JSON type changed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Retyped {
    /// The path, a JSON Pointer with `*` for any array element.
    pub path: String,
    /// The types it had in the earlier run, `|`-joined: `number|null`.
    pub before: String,
    /// The types it has in the later run.
    pub after: String,
}

/// How one api's JSON responses with one status changed shape between two runs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShapeChange {
    /// The api's full ID.
    pub api_id: String,
    /// The response status.
    pub status: u16,
    /// Paths only the later run's bodies have.
    pub added: Vec<String>,
    /// Paths only the earlier run's bodies have.
    pub removed: Vec<String>,
    /// Paths in both whose types differ.
    pub retyped: Vec<Retyped>,
}

/// The api and status of a response.
type Answer = (String, u16);

/// The shapes that changed from the `exchanges.0` responses to the `exchanges.1` ones, for
/// each api and status with kept JSON bodies in both. A truncated or non-JSON body is left
/// out, and a path under an array is compared only when the array has elements in both.
#[must_use]
pub fn shape_changes((before, after): (&[ExchangeRow], &[ExchangeRow])) -> Vec<ShapeChange> {
    let (earlier, later) = (shapes(before), shapes(after));
    later.iter().filter_map(|(key, shape)| earlier.get(key).and_then(|was| change(key, (was, shape)))).collect()
}

fn shapes(exchanges: &[ExchangeRow]) -> Shapes {
    let mut shapes = Shapes::new();
    for exchange in exchanges {
        if let Some((key, body)) = json_body(exchange) {
            add(shapes.entry(key).or_default(), &body);
        }
    }
    shapes
}

/// The exchange's api, status and whole JSON response body, when it has all three.
fn json_body(exchange: &ExchangeRow) -> Option<(Answer, Value)> {
    let (api, status) = (exchange.api_id.clone()?, exchange.status?);
    let body = exchange.response_body.as_deref().filter(|_| !exchange.body_truncated)?;
    Some(((api, status), serde_json::from_slice(body).ok()?))
}

/// Adds every path of `body` and its type to `shape`.
fn add(shape: &mut Shape, body: &Value) {
    let mut pending = vec![(String::new(), body)];
    while let Some((path, value)) = pending.pop() {
        match value {
            Value::Object(fields) => {
                pending.extend(fields.iter().map(|(name, field)| (format!("{path}/{}", segment(name)), field)));
            }
            Value::Array(items) => pending.extend(items.iter().map(|item| (format!("{path}/{ELEMENT}"), item))),
            _ => {}
        }
        shape.entry(path).or_default().insert(kind(value));
    }
}

/// A field name as a JSON Pointer segment (`~` as `~0`, `/` as `~1`), on one line.
fn segment(name: &str) -> String {
    one_line(&name.replace('~', "~0").replace('/', "~1")).into_owned()
}

const fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// How `shapes.1` differs from `shapes.0`, if it does.
fn change((api, status): &Answer, (before, after): (&Shape, &Shape)) -> Option<ShapeChange> {
    let only = |from: &Shape, other: &Shape| -> Vec<String> {
        from.keys().filter(|path| !other.contains_key(*path) && comparable(path, other)).cloned().collect()
    };
    let retyped: Vec<Retyped> = after
        .iter()
        .filter_map(|(path, types)| before.get(path).filter(|was| *was != types).map(|was| (path, was, types)))
        .map(|(path, was, types)| Retyped { path: path.clone(), before: joined(was), after: joined(types) })
        .collect();
    let (added, removed) = (only(after, before), only(before, after));
    let changed = !(added.is_empty() && removed.is_empty() && retyped.is_empty());
    changed.then(|| ShapeChange { api_id: api.clone(), status: *status, added, removed, retyped })
}

/// Whether `other` saw elements of every array `path` is under, so `path`'s absence there is
/// a change and not an empty array.
fn comparable(path: &str, other: &Shape) -> bool {
    let mut array = String::new();
    for part in path.split('/').skip(1) {
        array.push('/');
        array.push_str(part);
        if part == ELEMENT && !other.contains_key(&array) {
            return false;
        }
    }
    true
}

fn joined(types: &BTreeSet<&'static str>) -> String {
    types.iter().copied().collect::<Vec<_>>().join("|")
}

#[cfg(test)]
#[path = "body_shape_tests.rs"]
mod tests;
