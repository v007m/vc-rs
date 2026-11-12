use indexmap::IndexMap;
use serde_json::{Value, json};

use super::{SchemaError, compile_schema};

fn holding(entries: &[(&str, Value)]) -> IndexMap<String, Value> {
    entries.iter().map(|(key, schema)| ((*key).to_owned(), schema.clone())).collect()
}

/// Where and why the schema `key` names does not compile, else `None`.
fn fault(schemas: &IndexMap<String, Value>, key: &str) -> Option<(String, String)> {
    match compile_schema(schemas, key) {
        Ok(_) => None,
        Err(SchemaError::Invalid { at, reason, .. }) => Some((at, reason)),
        Err(other) => panic!("{other}"),
    }
}

#[test]
fn an_invalid_keyword_is_named_by_its_pointer_in_the_schema() {
    let schemas = holding(&[
        ("a.test.yaml:3:7", json!({"type": "object", "properties": {"age": {"type": "integr"}}})),
        ("b.test.yaml:3:7", json!({"minLength": -1})),
        ("c.test.yaml:3:7", json!({"pattern": "("})),
    ]);
    let at = |key| fault(&schemas, key).map(|(at, _)| at);
    assert_eq!(at("a.test.yaml:3:7").as_deref(), Some("/properties/age/type"));
    assert_eq!(at("b.test.yaml:3:7").as_deref(), Some("/minLength"));
    assert_eq!(at("c.test.yaml:3:7").as_deref(), Some("/pattern"));
}

#[test]
fn a_ref_that_leads_nowhere_or_outside_the_file_breaks_the_whole_schema() {
    let schemas = holding(&[
        ("nowhere.json", json!({"$ref": "#/definitions/gone"})),
        ("remote.json", json!({"$ref": "https://schemas.example/user.json"})),
    ]);
    let (at, reason) = fault(&schemas, "nowhere.json").unwrap();
    assert_eq!((at.as_str(), reason.contains("/definitions/gone")), ("", true), "{reason}");
    let remote = fault(&schemas, "remote.json").map(|(_, reason)| reason);
    let fetched = "`$ref` `https://schemas.example/user.json` is outside the file, and vc never fetches one";
    assert_eq!(remote.as_deref(), Some(fetched));
}

#[test]
fn a_pointer_compiles_only_the_part_it_names_with_the_files_own_refs() {
    let document = json!({
        "definitions": {
            "id": {"type": "string"},
            "user": {"type": "object", "properties": {"id": {"$ref": "#/definitions/id"}}},
            "broken": {"type": 5},
        },
    });
    let schemas = holding(&[("schemas/user.json", document)]);
    let user = compile_schema(&schemas, "schemas/user.json#/definitions/user").unwrap();
    assert_eq!(user.breaks(&json!({"id": "u1"})), Vec::<String>::new());
    assert_eq!(user.breaks(&json!({"id": 7})).len(), 1);
    let broken = fault(&schemas, "schemas/user.json#/definitions/broken").map(|(at, _)| at);
    assert_eq!(broken.as_deref(), Some("/definitions/broken/type"));
    assert!(fault(&schemas, "schemas/user.json#/definitions/gone").is_some());
}

#[test]
fn a_key_with_nothing_held_is_unloaded() {
    let schemas = holding(&[("schemas/user.json", json!({}))]);
    for key in ["schemas/gone.json", "schemas/gone.json#/x", "${var.schema}"] {
        let error = compile_schema(&schemas, key).unwrap_err();
        assert_eq!(error, SchemaError::Unloaded { key: key.to_owned() });
        assert_eq!(error.to_string(), format!("no schema is loaded for `{key}`"));
    }
}

#[test]
fn a_body_break_names_its_path_and_never_the_bodys_value() {
    let schemas = holding(&[("s.json", json!({"properties": {"name": {"type": "string"}}, "required": ["id"]}))]);
    let mut breaks = compile_schema(&schemas, "s.json").unwrap().breaks(&json!({"name": 4711}));
    breaks.sort();
    assert_eq!(breaks.len(), 2, "{breaks:?}");
    assert!(breaks[0].starts_with("/: ") && breaks[1].starts_with("/name: "), "{breaks:?}");
    assert!(!breaks.join(" ").contains("4711"), "{breaks:?}");
}
