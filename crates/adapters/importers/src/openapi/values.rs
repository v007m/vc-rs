//! Values for a draft from JSON Schemas: a parameter's default, a body field's generator.

use indexmap::IndexMap;
use serde_json::{Map, Value, json};

/// How many `$ref`s in a row are followed before giving up on a schema.
const MAX_HOPS: usize = 16;

/// String formats and the generator that fits each.
const FORMATS: [(&str, &str); 6] = [
    ("email", "${fake.internet.email}"),
    ("uuid", "${fake.string.uuid}"),
    ("date-time", "${fake.date.recent}"),
    ("date", "${fake.date.recent}"),
    ("uri", "https://example.com"),
    ("hostname", "example.com"),
];

/// Words in a field's name and the generator that fits a string field so named.
const NAMES: [(&str, &str); 5] = [
    ("email", "${fake.internet.email}"),
    ("phone", "${fake.phone.number}"),
    ("city", "${fake.location.city}"),
    ("country", "${fake.location.countryCode}"),
    ("name", "${fake.person.fullName}"),
];

/// `schema` with its local `$ref`s followed, as far as they lead.
pub fn resolve<'a>(doc: &'a Value, schema: &'a Value) -> &'a Value {
    let mut current = schema;
    for _ in 0..MAX_HOPS {
        let pointer = current.get("$ref").and_then(Value::as_str).and_then(|at| at.strip_prefix('#'));
        match pointer.and_then(|pointer| doc.pointer(pointer)) {
            Some(next) => current = next,
            None => return current,
        }
    }
    current
}

/// The value the schema shows or defaults to: its first example, `example`, `default`,
/// `const` or first `enum` value.
pub fn sample(schema: &Value) -> Option<Value> {
    let first = schema.get("examples").and_then(Value::as_array).and_then(|examples| examples.first());
    let shown = ["example", "default", "const"].iter().find_map(|key| schema.get(*key));
    let listed = schema.get("enum").and_then(Value::as_array).and_then(|choices| choices.iter().find(|c| !c.is_null()));
    first.or(shown).or(listed).cloned()
}

/// The value a field named `name` gets in a draft: what its schema shows, else a generator
/// for its type and format.
pub fn generated(doc: &Value, name: &str, schema: &Value) -> Value {
    let schema = alternative(doc, resolve(doc, schema));
    if let Some(sample) = sample(schema) {
        return sample;
    }
    let typed = ty(schema).and_then(|ty| GENERATORS.iter().find(|(name, _)| *name == ty));
    typed.map_or_else(|| string(name, schema), |(_, generate)| generate(schema))
}

/// The value of a field of each non-string schema type; any other type is a string.
const GENERATORS: [(&str, Generator); 5] = [
    ("integer", integer),
    ("number", |_| json!("${fake.number.float(0, 100)}")),
    ("boolean", |_| json!(true)),
    ("array", |_| json!([])),
    ("object", |_| json!({})),
];

/// The draft value of a field, from its schema.
type Generator = fn(&Value) -> Value;

/// An object schema's properties and its required fields.
type Fields = (Map<String, Value>, Vec<String>);

/// The body a request sends for `schema`, and the vars it reads: each required field of an
/// object is `${var.<field>}`, with the field's generated value as the var's default.
pub fn body(doc: &Value, schema: &Value) -> (Value, IndexMap<String, Value>) {
    let schema = alternative(doc, resolve(doc, schema));
    let Some((properties, required)) = object(doc, schema) else {
        return (generated(doc, "body", schema), IndexMap::new());
    };
    let mut body = Map::new();
    let mut vars = IndexMap::new();
    for field in required {
        let Some(field_schema) = properties.get(&field) else { continue };
        let var = crate::openapi::naming::camel(&field);
        vars.insert(var.clone(), generated(doc, &field, field_schema));
        body.insert(field, json!(format!("${{var.{var}}}")));
    }
    (body.into(), vars)
}

/// The properties and required fields of an object schema, `allOf` parts merged.
fn object(doc: &Value, schema: &Value) -> Option<Fields> {
    let parts = schema.get("allOf").and_then(Value::as_array);
    if ty(schema) != Some("object") && schema.get("properties").is_none() && parts.is_none() {
        return None;
    }
    let mut properties = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut required = names(schema.get("required"));
    for part in parts.into_iter().flatten() {
        if let Some((more, mut needed)) = object(doc, resolve(doc, part)) {
            properties.extend(more);
            needed.retain(|field| !required.contains(field));
            required.extend(needed);
        }
    }
    Some((properties, required))
}

fn names(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect()
}

/// The first alternative of an `anyOf` or `oneOf` that is not `null`; the schema itself
/// otherwise.
fn alternative<'a>(doc: &'a Value, schema: &'a Value) -> &'a Value {
    let choices = ["anyOf", "oneOf"].iter().find_map(|key| schema.get(*key).and_then(Value::as_array));
    let chosen =
        choices.into_iter().flatten().map(|choice| resolve(doc, choice)).find(|choice| ty(choice) != Some("null"));
    chosen.unwrap_or(schema)
}

/// The schema's type, the first that is not `null` when it lists several.
fn ty(schema: &Value) -> Option<&str> {
    match schema.get("type") {
        Some(Value::String(ty)) => Some(ty),
        Some(Value::Array(types)) => types.iter().filter_map(Value::as_str).find(|ty| *ty != "null"),
        _ => None,
    }
}

fn integer(schema: &Value) -> Value {
    let bound = |inclusive: &str, exclusive: &str, step: i64| {
        let exclusive = schema.get(exclusive).and_then(Value::as_i64).map(|limit| limit.saturating_add(step));
        exclusive.or_else(|| schema.get(inclusive).and_then(Value::as_i64))
    };
    let min = bound("minimum", "exclusiveMinimum", 1).unwrap_or(1);
    let max = bound("maximum", "exclusiveMaximum", -1).unwrap_or_else(|| min.saturating_add(99));
    json!(format!("${{fake.number.int({min}, {max})}}"))
}

fn string(name: &str, schema: &Value) -> Value {
    let format = schema.get("format").and_then(Value::as_str);
    let by_format = FORMATS.iter().find(|(known, _)| Some(*known) == format);
    let lower = name.to_ascii_lowercase();
    let by_name = NAMES.iter().find(|(word, _)| lower.contains(word));
    let generator = by_format.or(by_name).map_or("${fake.lorem.word}", |(_, generator)| generator);
    json!(generator)
}

#[cfg(test)]
mod tests;
