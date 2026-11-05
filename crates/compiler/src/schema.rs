//! JSON Schemas for editors (`vc schema export`), generated from the shape declarations the
//! shape-check uses, so the editors and the checker agree.

use serde_json::{Map, Value, json};
use vc_model::{FileKind, SourceFileId};
use vc_parser::{Node, ScalarKind, Syntax};

use crate::shapes::{self, Key, Shape, Ty};

/// The JSON Schema of one kind of definition file, for editors (`vc schema export`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileSchema {
    /// The kind of file the schema describes.
    pub kind: FileKind,
    /// The schema itself (JSON Schema 2020-12).
    pub schema: Value,
}

/// Where the published schemas live; `<kind>.schema.json` is appended.
const BASE_URL: &str = "https://raw.githubusercontent.com/v007m/vc-rs/master/schema/";

/// One schema per [`FileKind`], in [`FileKind::ALL`] order.
#[must_use]
pub fn schemas() -> Vec<FileSchema> {
    FileKind::ALL.into_iter().map(|kind| FileSchema { kind, schema: schema_of(kind) }).collect()
}

fn schema_of(kind: FileKind) -> Value {
    let root = shapes::root(kind);
    let mut defs = Map::new();
    for shape in reachable_defs(root) {
        defs.insert(shape.title.to_owned(), object(shape));
    }
    let mut schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": format!("{BASE_URL}{}.schema.json", kind.name()),
        "title": format!("Vibranium {} file", kind.name()),
    });
    merge(&mut schema, object(root));
    merge(&mut schema, json!({ "$defs": defs }));
    schema
}

/// Every shape the root refers to, which become `$defs`.
fn reachable_defs(root: &'static Shape) -> impl Iterator<Item = &'static Shape> {
    shapes::reachable(root).into_iter().skip(1)
}

fn merge(into: &mut Value, from: Value) {
    if let (Value::Object(into), Value::Object(from)) = (into, from) {
        into.extend(from);
    }
}

/// A shape as a closed object: its keys, the required ones, and its one-of group.
fn object(shape: &Shape) -> Value {
    let properties: Map<String, Value> = shape.all_keys().map(|key| (key.name.to_owned(), property(key))).collect();
    let required: Vec<&str> = shape.all_keys().filter(|key| key.required).map(|key| key.name).collect();
    let mut schema = json!({
        "type": "object",
        "description": shape.doc,
        "properties": properties,
        "additionalProperties": false,
    });
    if !required.is_empty() {
        merge(&mut schema, json!({ "required": required }));
    }
    if let Some(group) = shape.one_of {
        let one_of: Vec<Value> = group.keys.iter().map(|key| json!({ "required": [key] })).collect();
        merge(&mut schema, json!({ "oneOf": one_of }));
    }
    schema
}

/// One key: its type, description, default and example.
fn property(key: &Key) -> Value {
    let mut schema = of_type(key.ty);
    merge(&mut schema, json!({ "description": key.doc }));
    if let Some(default) = key.default.and_then(yaml).filter(|node| fits(key.ty, node)) {
        merge(&mut schema, json!({ "default": to_json(&default) }));
    }
    if let Some(example) = key.example.and_then(yaml) {
        merge(&mut schema, json!({ "examples": [to_json(&example)] }));
    }
    schema
}

fn of_type(ty: Ty) -> Value {
    match ty {
        Ty::Int => json!({ "type": "integer" }),
        Ty::Bool => json!({ "type": "boolean" }),
        Ty::Enum(words) => json!({ "enum": words }),
        Ty::Schema => json!({ "type": ["object", "string"] }),
        Ty::Status => json!({ "type": ["integer", "string", "array", "object"] }),
        Ty::Date => json!({ "type": "string", "format": "date" }),
        Ty::Value | Ty::Matcher(_) => json!({}),
        Ty::List(item) => json!({ "type": "array", "items": of_type(*item) }),
        Ty::MapOf(value) => json!({ "type": "object", "additionalProperties": of_type(*value) }),
        Ty::Obj(shape) => json!({ "$ref": format!("#/$defs/{}", shape().title) }),
        Ty::Either(alternatives) => json!({ "anyOf": alternatives.iter().map(|ty| of_type(*ty)).collect::<Vec<_>>() }),
        Ty::Text | Ty::Id | Ty::Ref | Ty::Template | Ty::Duration | Ty::Size | Ty::Cel => json!({ "type": "string" }),
    }
}

/// A default or example, read as YAML.
fn yaml(text: &str) -> Option<Node> {
    vc_parser::parse(SourceFileId::new(0), Syntax::Yaml, text).0
}

/// Whether a declared default is a real value of the key's type rather than a description
/// such as "the file's"; text defaults are always left to the description.
fn fits(ty: Ty, node: &Node) -> bool {
    let kind = node.as_scalar().map(|scalar| scalar.kind);
    let span = node.span;
    match ty {
        Ty::Int => kind == Some(ScalarKind::Int),
        Ty::Bool => kind == Some(ScalarKind::Bool),
        Ty::Enum(words) => node.as_str().is_some_and(|text| words.contains(&text)),
        Ty::Duration => node.as_str().is_some_and(|text| vc_template::parse_duration(text, span).is_ok()),
        Ty::Size => node.as_str().is_some_and(|text| vc_template::parse_size(text, span).is_ok()),
        Ty::List(item) => node.as_scalar().is_none() && node.items().iter().all(|node| fits(*item, node)),
        Ty::Obj(_) => !node.entries().is_empty(),
        Ty::Either(alternatives) => alternatives.iter().any(|ty| fits(*ty, node)),
        _ => false,
    }
}

/// A YAML value as JSON, with YAML 1.2 core types.
pub fn to_json(node: &Node) -> Value {
    match &node.value {
        vc_parser::Value::Seq(items) => items.iter().map(to_json).collect(),
        vc_parser::Value::Map(entries) => {
            entries.iter().map(|entry| (entry.key.clone(), to_json(&entry.value))).collect::<Map<_, _>>().into()
        }
        vc_parser::Value::Scalar(scalar) => scalar_json(scalar.kind, &scalar.text),
    }
}

fn scalar_json(kind: ScalarKind, text: &str) -> Value {
    let typed = match kind {
        ScalarKind::Null => Some(Value::Null),
        ScalarKind::Bool => Some(Value::Bool(text.eq_ignore_ascii_case("true"))),
        ScalarKind::Int => text.parse::<i64>().ok().map(Value::from),
        ScalarKind::Float => text.parse::<f64>().ok().map(Value::from),
        ScalarKind::String => None,
    };
    typed.unwrap_or_else(|| Value::String(text.to_owned()))
}

#[cfg(test)]
mod tests;
