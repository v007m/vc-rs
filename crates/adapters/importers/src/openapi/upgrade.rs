//! `OpenAPI` 3.0 to 3.1: the schema keywords that JSON Schema 2020-12 spells differently.
//!
//! `nullable: true` adds `"null"` to `type` (or wraps a schema without one in
//! `anyOf: [..., {type: null}]`), a boolean `exclusiveMinimum`/`exclusiveMaximum` takes the
//! bound from `minimum`/`maximum`, and `example` becomes `examples: [...]`. Only schemas
//! change: an `example` on a media type or parameter is still 3.1.

use serde_json::{Map, Value, json};

/// The keys whose value is one subschema.
const ONE: [&str; 5] = ["items", "additionalProperties", "not", "contains", "propertyNames"];
/// The keys whose value is a list of subschemas.
const LIST: [&str; 4] = ["allOf", "anyOf", "oneOf", "prefixItems"];
/// The keys whose value maps names to subschemas.
const NAMED: [&str; 3] = ["properties", "patternProperties", "$defs"];
/// The bounds a boolean exclusive keyword reads its number from.
const BOUNDS: [(&str, &str); 2] = [("exclusiveMinimum", "minimum"), ("exclusiveMaximum", "maximum")];

/// `doc`, a 3.0 document in the 3.0 layout, as 3.1.
pub fn to_openapi31(mut doc: Map<String, Value>) -> Map<String, Value> {
    for (key, value) in &mut doc {
        if key == "components" { components(value) } else { outside_schemas(value) }
    }
    doc.insert("openapi".to_owned(), json!("3.1.0"));
    doc
}

/// `components.schemas` holds schemas; the other sections hold them under `schema` keys.
fn components(value: &mut Value) {
    let Value::Object(sections) = value else { return };
    for (name, section) in sections.iter_mut() {
        match section {
            Value::Object(schemas) if name == "schemas" => schemas.values_mut().for_each(schema),
            _ => outside_schemas(section),
        }
    }
}

/// Finds the schemas in a part of the document that is not one: each `schema` value.
fn outside_schemas(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if key == "schema" { schema(value) } else { outside_schemas(value) }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(outside_schemas),
        _ => {}
    }
}

/// Upgrades one schema and every subschema inside it.
fn schema(value: &mut Value) {
    let Value::Object(map) = value else { return };
    for key in ONE {
        if let Some(sub) = map.get_mut(key) {
            schema(sub);
        }
    }
    for key in LIST {
        if let Some(Value::Array(subs)) = map.get_mut(key) {
            subs.iter_mut().for_each(schema);
        }
    }
    for key in NAMED {
        if let Some(Value::Object(subs)) = map.get_mut(key) {
            subs.values_mut().for_each(schema);
        }
    }
    exclusive_bounds(map);
    if let Some(example) = map.remove("example") {
        map.entry("examples").or_insert_with(|| json!([example]));
    }
    nullable(value);
}

fn exclusive_bounds(map: &mut Map<String, Value>) {
    for (exclusive, bound) in BOUNDS {
        match map.get(exclusive) {
            Some(Value::Bool(true)) => {
                let limit = map.remove(bound).unwrap_or(Value::Null);
                map.insert(exclusive.to_owned(), limit);
            }
            Some(Value::Bool(false)) => {
                map.remove(exclusive);
            }
            _ => {}
        }
    }
}

fn nullable(value: &mut Value) {
    let Value::Object(map) = value else { return };
    let marked = [map.remove("nullable"), map.remove("x-nullable")].contains(&Some(Value::Bool(true)));
    if !marked {
        return;
    }
    if let Some(Value::Array(choices)) = map.get_mut("enum")
        && !choices.contains(&Value::Null)
    {
        choices.push(Value::Null);
    }
    match map.get_mut("type") {
        Some(Value::String(ty)) => {
            let ty = std::mem::take(ty);
            map.insert("type".to_owned(), json!([ty, "null"]));
        }
        Some(Value::Array(types)) if !types.contains(&json!("null")) => types.push(json!("null")),
        Some(_) => {}
        None => *value = json!({ "anyOf": [std::mem::take(value), { "type": "null" }] }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::to_openapi31;

    fn upgraded(doc: Value) -> Value {
        let Value::Object(doc) = doc else { return Value::Null };
        Value::Object(to_openapi31(doc))
    }

    #[test]
    fn nullable_types_gain_null() {
        let doc = upgraded(json!({
            "openapi": "3.0.3",
            "components": { "schemas": {
                "Name": { "type": "string", "nullable": true },
                "Role": { "type": "string", "enum": ["admin"], "nullable": true },
                "Ref": { "allOf": [{ "$ref": "#/components/schemas/Name" }], "nullable": true },
                "Kept": { "type": "string", "nullable": false },
            }},
        }));
        let schemas = &doc["components"]["schemas"];
        assert_eq!(schemas["Name"], json!({ "type": ["string", "null"] }));
        assert_eq!(schemas["Role"], json!({ "type": ["string", "null"], "enum": ["admin", null] }));
        let wrapped = json!({ "anyOf": [{ "allOf": [{ "$ref": "#/components/schemas/Name" }] }, { "type": "null" }] });
        assert_eq!(schemas["Ref"], wrapped);
        assert_eq!(schemas["Kept"], json!({ "type": "string" }));
        assert_eq!(doc["openapi"], "3.1.0");
    }

    #[test]
    fn boolean_bounds_take_their_number() {
        let doc = upgraded(json!({ "components": { "schemas": { "Age": {
            "type": "integer", "minimum": 0, "exclusiveMinimum": true, "maximum": 9, "exclusiveMaximum": false,
        }}}}));
        assert_eq!(
            doc["components"]["schemas"]["Age"],
            json!({ "type": "integer", "exclusiveMinimum": 0, "maximum": 9 })
        );
    }

    #[test]
    fn schema_examples_become_lists_and_media_examples_stay() {
        let doc = upgraded(json!({ "paths": { "/users": { "post": { "requestBody": { "content": {
            "application/json": {
                "example": { "email": "a@b.c" },
                "schema": { "type": "object", "properties": {
                    "email": { "type": "string", "example": "a@b.c" },
                    "tags": { "type": "array", "items": { "type": "string", "nullable": true } },
                }},
            },
        }}}}}}));
        let media = &doc["paths"]["/users"]["post"]["requestBody"]["content"]["application/json"];
        assert_eq!(media["example"], json!({ "email": "a@b.c" }));
        let properties = &media["schema"]["properties"];
        assert_eq!(properties["email"], json!({ "type": "string", "examples": ["a@b.c"] }));
        assert_eq!(properties["tags"]["items"], json!({ "type": ["string", "null"] }));
    }

    #[test]
    fn schemas_in_other_components_are_upgraded() {
        let doc = upgraded(json!({ "components": { "parameters": { "Limit": {
            "name": "limit", "in": "query", "schema": { "type": "integer", "nullable": true },
        }}}}));
        assert_eq!(doc["components"]["parameters"]["Limit"]["schema"], json!({ "type": ["integer", "null"] }));
    }
}
