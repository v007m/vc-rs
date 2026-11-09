//! Pieces of the hand-written tool input schemas (D197).

use serde_json::{Value, json};

/// An object schema with `properties`, of which `required` must be given.
pub fn object(properties: Value, required: &[&str]) -> Value {
    let mut schema = json!({ "type": "object", "required": required, "additionalProperties": false });
    schema["properties"] = properties;
    schema
}

/// A list-of-strings property.
pub fn strings(description: &str) -> Value {
    json!({ "type": "array", "items": {"type": "string"}, "description": description })
}
