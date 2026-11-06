use serde_json::{Value, json};
use vc_model::FileKind;

use crate::shapes;
use crate::shapes::{Shape, Ty, Unit};

fn config() -> &'static Shape {
    shapes::root(FileKind::Config)
}

fn schema(kind: FileKind) -> Value {
    super::schemas().into_iter().find(|schema| schema.kind == kind).unwrap().schema
}

#[test]
fn has_one_object_schema_per_kind() {
    let all = super::schemas();
    assert_eq!(all.iter().map(|s| s.kind).collect::<Vec<_>>(), FileKind::ALL);
    for schema in all {
        let schema = schema.schema;
        assert_eq!(schema["$schema"], "https://json-schema.org/draft/2020-12/schema");
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        let id = schema["$id"].as_str().unwrap();
        assert!(id.starts_with("https://raw.githubusercontent.com/v007m/vc-rs/master/schema/"), "{id}");
    }
    assert!(schema(FileKind::Test)["$id"].as_str().unwrap().ends_with("/test.schema.json"));
}

#[test]
fn a_created_file_names_its_schema_on_its_first_line() {
    let line = super::schema_line(FileKind::Api);
    assert_eq!(line, format!("# yaml-language-server: $schema={}", schema(FileKind::Api)["$id"].as_str().unwrap()));
}

#[test]
fn describes_keys_with_types_defaults_and_examples() {
    let config = schema(FileKind::Config);
    assert_eq!(config["required"], json!(["syntaxVersion"]));
    let version = &config["properties"]["syntaxVersion"];
    assert_eq!(version["type"], "integer");
    assert_eq!(version["examples"], json!([2]));
    assert_eq!(version["description"], "File-format version");
    let defaults = &config["$defs"]["defaults"]["properties"];
    assert_eq!(defaults["timeout"], json!({ "type": "string", "description": "Per request", "default": "30s" }));
    assert_eq!(defaults["concurrency"]["default"], 16);
    assert!(config["properties"]["project"].get("default").is_none(), "a described default stays out");
}

#[test]
fn refers_to_nested_shapes_through_defs() {
    let api = schema(FileKind::Api);
    assert_eq!(api["properties"]["apis"]["items"], json!({ "$ref": "#/$defs/apiEntry" }));
    let entry = &api["$defs"]["apiEntry"];
    assert_eq!(entry["required"], json!(["id", "path"]));
    assert_eq!(entry["properties"]["method"]["enum"][0], "GET");
    assert_eq!(entry["properties"]["expect"]["default"], json!({ "status": "2xx" }));
    assert!(api["$defs"].get("api").is_none(), "the root is not repeated in $defs");
    let test = schema(FileKind::Test);
    assert_eq!(test["$defs"]["testEntry"]["oneOf"], json!([{ "required": ["call"] }, { "required": ["steps"] }]));
    let action = &test["$defs"]["action"];
    assert_eq!(action["oneOf"].as_array().unwrap().len(), 4);
    assert_eq!(action["properties"]["auth"]["anyOf"][1], json!({ "$ref": "#/$defs/authUse" }));
}

#[test]
fn keeps_only_defaults_that_are_values_of_the_type() {
    let fit = |ty, text| super::fits(ty, &super::yaml(text).unwrap());
    assert!(fit(Ty::Int, "8") && !fit(Ty::Int, "eight"));
    assert!(fit(Ty::Bool, "false") && !fit(Ty::Bool, "'false'"));
    assert!(fit(Ty::Enum(&["api", "full"]), "api") && !fit(Ty::Enum(&["api"]), "all"));
    assert!(fit(Ty::Duration, "100ms") && !fit(Ty::Duration, "defaults.timeout"));
    assert!(fit(Ty::Size, "1mb") && !fit(Ty::Size, "big"));
    assert!(fit(Ty::List(&Ty::Int), "[401]") && fit(Ty::List(&Ty::Int), "[]") && !fit(Ty::List(&Ty::Int), "x"));
    assert!(fit(Ty::Either(&[Ty::Int, Ty::Duration]), "0s"));
    assert!(fit(Ty::Obj(config), "a: 1") && !fit(Ty::Obj(config), "the file's"));
    assert!(!fit(Ty::Text, "varlock") && !fit(Ty::Matcher(Unit::Plain), "1"));
}

#[test]
fn reads_yaml_values_as_json() {
    let json = |text| super::to_json(&super::yaml(text).unwrap());
    assert_eq!(json("{a: [1, 2.5, true, null, x], b: '3'}"), json!({ "a": [1, 2.5, true, null, "x"], "b": "3" }));
    assert_eq!(json("0x1F"), json!("0x1F"));
}
