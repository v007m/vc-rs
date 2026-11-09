//! JSON values as block YAML in the house style: two-space indent, text plain unless YAML
//! needs quotes, `{}` and `[]` for empty collections. Used for generated files, whose values
//! come from a foreign spec rather than from a document being edited.

use std::fmt::Write as _;

use serde_json::Value;
use vc_parser::write_text;

/// `key: value` at `indent`, a collection on the lines below it.
pub fn entry(out: &mut String, indent: usize, (key, value): (&str, &Value)) {
    let pad = " ".repeat(indent);
    let key = write_text(key, false);
    if nested(value) {
        let _ = writeln!(out, "{pad}{key}:");
        body(out, indent + 2, value);
    } else {
        let _ = writeln!(out, "{pad}{key}: {}", scalar(value));
    }
}

/// The entries of a map, or the items of a list, at `indent`.
pub fn body(out: &mut String, indent: usize, value: &Value) {
    match value {
        Value::Object(map) => map.iter().for_each(|(key, value)| entry(out, indent, (key.as_str(), value))),
        Value::Array(items) => items.iter().for_each(|value| item(out, indent, value)),
        _ => {}
    }
}

/// A list item at `indent` made of `fields` in their order (a JSON map's keys are sorted):
/// `- id: …` then the rest below it.
pub fn fields_item(out: &mut String, indent: usize, fields: &[(&str, Value)]) {
    let mut inner = String::new();
    for (key, value) in fields {
        entry(&mut inner, indent + 2, (*key, value));
    }
    dashed(out, indent, &inner);
}

/// `- value` at `indent`; a collection starts on the dash's line.
fn item(out: &mut String, indent: usize, value: &Value) {
    if !nested(value) {
        let _ = writeln!(out, "{}- {}", " ".repeat(indent), scalar(value));
        return;
    }
    let mut inner = String::new();
    body(&mut inner, indent + 2, value);
    dashed(out, indent, &inner);
}

/// `inner`, written two deeper than `indent`, with a dash in its first line's indent.
fn dashed(out: &mut String, indent: usize, inner: &str) {
    out.push_str(&" ".repeat(indent));
    out.push_str("- ");
    out.push_str(inner.get(indent + 2..).unwrap_or_default());
}

fn nested(value: &Value) -> bool {
    match value {
        Value::Object(map) => !map.is_empty(),
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(text) => write_text(text, false),
        Value::Object(_) => "{}".to_owned(),
        Value::Array(_) => "[]".to_owned(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{body, entry, fields_item};

    #[test]
    fn an_item_keeps_its_fields_in_order() {
        let mut out = String::new();
        fields_item(&mut out, 2, &[("id", json!("b")), ("call", json!("x#a")), ("expect", json!({ "status": 201 }))]);
        assert_eq!(out, "  - id: b\n    call: x#a\n    expect:\n      status: 201\n");
    }

    #[test]
    fn values_are_written_as_block_yaml() {
        let mut out = String::new();
        entry(&mut out, 0, ("vars", &json!({ "id": 7, "name": "${fake.person.fullName}", "tag": null })));
        assert_eq!(out, "vars:\n  id: 7\n  name: ${fake.person.fullName}\n  tag: null\n");
        let mut out = String::new();
        body(&mut out, 2, &json!([{ "a": "x", "b": [1, []] }, [true], {}]));
        assert_eq!(out, "  - a: x\n    b:\n      - 1\n      - []\n  - - true\n  - {}\n");
    }

    #[test]
    fn text_yaml_would_misread_is_quoted() {
        let mut out = String::new();
        entry(&mut out, 0, ("200", &json!({ "description": "true", "$ref": "#/a" })));
        assert!(out.starts_with("\"200\":\n"), "{out}");
        assert!(out.contains("description: \"true\"\n") && out.contains("\"#/a\""), "{out}");
    }
}
