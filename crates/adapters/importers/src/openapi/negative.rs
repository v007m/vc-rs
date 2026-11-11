//! The rows of the test `--negative` adds: the api's drafted body with one field missing, of
//! the wrong type or outside its enum, one row each.

use serde_json::{Map, Value, json};
use vc_interfaces::{DraftApi, DraftExample};

use super::draft::{Reader, Route};
use super::values::{fields, shape, types};

/// A value of the wrong type for a field of each schema type, as JSON.
const WRONG: [(&str, &str); 6] = [
    ("string", "123"),
    ("integer", "\"not a number\""),
    ("number", "\"not a number\""),
    ("boolean", "\"yes\""),
    ("array", "\"not a list\""),
    ("object", "\"not an object\""),
];

/// The value sent for a field outside its enum.
const UNLISTED: &str = "not-a-listed-value";

/// One way to break a field: what the row is called after the field's name and the value it
/// sends, `None` to leave the field out; none when the way does not fit the field.
type Breaker = fn(&Field<'_>) -> Option<(String, Option<Value>)>;

/// The ways to break a field, in the order the rows are written.
const BREAKERS: [Breaker; 3] = [missing, mistyped, unlisted];

/// A field of the drafted body, as the body's schema describes it.
struct Field<'s> {
    schema: &'s Value,
    /// The JSON types it accepts; none when it accepts any.
    types: Option<Vec<&'s str>>,
    required: bool,
}

impl Reader<'_> {
    /// The rows for `api`: for each field its JSON body sends that the schema describes, a
    /// body for each way to break it. None for an api without such a body.
    pub fn invalid_bodies(&self, route: &Route<'_>, api: &DraftApi) -> Vec<DraftExample> {
        let sent = api.body.as_ref().and_then(Value::as_object).filter(|_| api.content_type.is_none());
        let described = self.body_schema(route).and_then(|schema| fields(self.doc, &schema));
        let (Some(sent), Some((properties, required))) = (sent, described) else { return Vec::new() };
        let sent = filled(sent, api);
        let mut rows = Vec::new();
        for (name, schema) in sent.keys().filter_map(|name| Some((name, properties.get(name)?))) {
            let field = Field {
                schema: shape(self.doc, schema),
                types: types(self.doc, schema),
                required: required.contains(name),
            };
            for (case, value) in BREAKERS.iter().filter_map(|breaker| breaker(&field)) {
                rows.push(DraftExample { name: format!("{name} {case}"), body: with(&sent, name, value) });
            }
        }
        rows
    }

    /// The schema of the operation's JSON request body.
    fn body_schema(&self, route: &Route<'_>) -> Option<Value> {
        let request = route.op.request_body(self.spec).ok().flatten()?;
        let (_, media) = request.content.iter().find(|(media, _)| media.contains("json"))?;
        serde_json::to_value(media).ok()?.get("schema").cloned()
    }
}

/// `body` with each field that reads one of the api's vars (`${var.email}`) holding that
/// var's value, as a row is sent without the api's vars.
fn filled(body: &Map<String, Value>, api: &DraftApi) -> Map<String, Value> {
    body.iter().map(|(name, value)| (name.clone(), var_value(value, api).unwrap_or(value).clone())).collect()
}

/// The value of the api var `value` reads when it is only that var (`${var.email}`).
fn var_value<'a>(value: &Value, api: &'a DraftApi) -> Option<&'a Value> {
    let var = value.as_str()?.strip_prefix("${var.")?.strip_suffix('}')?;
    api.vars.get(var)
}

/// `body` with `name` holding `value`, or without it for `None`.
fn with(body: &Map<String, Value>, name: &str, value: Option<Value>) -> Value {
    let mut body = body.clone();
    body.remove(name);
    body.extend(value.map(|value| (name.to_owned(), value)));
    Value::Object(body)
}

fn missing(field: &Field<'_>) -> Option<(String, Option<Value>)> {
    field.required.then(|| ("missing".to_owned(), None))
}

/// A value of none of the field's types: the first of [`WRONG`]'s for its types, in order,
/// that no type accepts.
fn mistyped(field: &Field<'_>) -> Option<(String, Option<Value>)> {
    let allowed = field.types.as_deref()?;
    let candidates = allowed.iter().filter_map(|ty| WRONG.iter().find(|(known, _)| known == ty));
    let mut values = candidates.filter_map(|(_, wrong)| serde_json::from_str::<Value>(wrong).ok());
    let wrong = values.find(|value| !allowed.iter().any(|ty| accepts(ty, value)))?;
    Some((format!("is not {}", listed(allowed)), Some(wrong)))
}

/// Whether a schema of type `ty` accepts `value`'s JSON type; a number accepts an integer.
fn accepts(ty: &str, value: &Value) -> bool {
    match value {
        Value::Null => ty == "null",
        Value::Bool(_) => ty == "boolean",
        Value::Number(number) if number.is_f64() => ty == "number",
        Value::Number(_) => matches!(ty, "integer" | "number"),
        Value::String(_) => ty == "string",
        Value::Array(_) => ty == "array",
        Value::Object(_) => ty == "object",
    }
}

/// The types a row names, `null` left out: `a string`, `an integer`, `a string or integer`,
/// `a string, integer or boolean`.
fn listed(types: &[&str]) -> String {
    let named: Vec<&str> = types.iter().copied().filter(|ty| *ty != "null").collect();
    let article = if named.first().is_some_and(|ty| ty.starts_with(['a', 'e', 'i', 'o', 'u'])) { "an" } else { "a" };
    let words = match named.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    };
    format!("{article} {words}")
}

fn unlisted(field: &Field<'_>) -> Option<(String, Option<Value>)> {
    let listed = field.schema.get("enum")?.as_array()?;
    let value = json!(UNLISTED);
    (!listed.contains(&value)).then(|| ("outside its enum".to_owned(), Some(value)))
}
