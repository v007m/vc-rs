use serde_json::{Value, json};

use super::{MAX_HOPS, body, generated, resolve, sample, types};

fn doc() -> Value {
    json!({ "components": { "schemas": {
        "User": {
            "type": "object",
            "required": ["email", "age"],
            "properties": {
                "email": { "type": "string", "format": "email" },
                "age": { "type": "integer", "exclusiveMinimum": 17, "maximum": 120 },
                "nickname": { "type": "string" },
            },
        },
        "Admin": { "allOf": [{ "$ref": "#/components/schemas/User" }, {
            "type": "object", "required": ["role"], "properties": { "role": { "$ref": "#/components/schemas/Role" } },
        }]},
        "Role": { "type": "string", "enum": ["admin", "member"] },
        "Alias": { "$ref": "#/components/schemas/Role" },
        "Loop": { "$ref": "#/components/schemas/Loop" },
    }}})
}

#[test]
fn references_are_followed_as_far_as_they_lead() {
    let doc = doc();
    assert_eq!(resolve(&doc, &json!({ "$ref": "#/components/schemas/Alias" }))["enum"][0], "admin");
    let dangling = json!({ "$ref": "#/components/schemas/Nope" });
    assert_eq!(resolve(&doc, &dangling), &dangling);
    assert!(resolve(&doc, &json!({ "$ref": "#/components/schemas/Loop" })).get("$ref").is_some());
}

#[test]
fn samples_come_from_what_the_schema_shows() {
    assert_eq!(sample(&json!({ "examples": [3], "default": 4 })), Some(json!(3)));
    assert_eq!(sample(&json!({ "default": 4, "enum": [5] })), Some(json!(4)));
    assert_eq!(sample(&json!({ "enum": [null, 5] })), Some(json!(5)));
    assert_eq!(sample(&json!({ "type": "string" })), None);
}

#[test]
fn string_generators_fit_format_and_name() {
    let doc = doc();
    let of = |name: &str, schema: Value| generated(&doc, name, &schema);
    assert_eq!(of("x", json!({ "type": "string", "format": "uuid" })), "${fake.string.uuid}");
    assert_eq!(of("contactEmail", json!({ "type": "string" })), "${fake.internet.email}");
    assert_eq!(of("fullName", json!({ "type": ["string", "null"] })), "${fake.person.fullName}");
    assert_eq!(of("x", json!({ "type": "string" })), "${fake.lorem.word}");
}

#[test]
fn other_generators_fit_type_and_bounds() {
    let doc = doc();
    let of = |name: &str, schema: Value| generated(&doc, name, &schema);
    assert_eq!(of("x", json!({ "type": "integer", "minimum": 5 })), "${fake.number.int(5, 104)}");
    assert_eq!(of("x", json!({ "type": "integer", "exclusiveMaximum": 10 })), "${fake.number.int(1, 9)}");
    assert_eq!(of("x", json!({ "type": "number" })), "${fake.number.float(0, 100)}");
    assert_eq!(of("x", json!({ "type": "boolean" })), true);
    assert_eq!(of("x", json!({ "type": "array" })), json!([]));
    assert_eq!(of("x", json!({ "anyOf": [{ "type": "null" }, { "type": "boolean" }] })), true);
    assert_eq!(of("x", json!({ "$ref": "#/components/schemas/Alias" })), "admin");
}

#[test]
fn bodies_send_required_fields_as_vars() {
    let doc = doc();
    let (sent, vars) = body(&doc, &json!({ "$ref": "#/components/schemas/User" }));
    assert_eq!(sent, json!({ "email": "${var.email}", "age": "${var.age}" }));
    let vars: Vec<_> = vars.into_iter().collect();
    assert_eq!(
        vars,
        [("email".into(), json!("${fake.internet.email}")), ("age".into(), json!("${fake.number.int(18, 120)}"))]
    );
}

#[test]
fn all_of_parts_are_merged() {
    let doc = doc();
    let (sent, vars) = body(&doc, &json!({ "$ref": "#/components/schemas/Admin" }));
    assert_eq!(sent, json!({ "email": "${var.email}", "age": "${var.age}", "role": "${var.role}" }));
    assert_eq!(vars.get("role"), Some(&json!("admin")));
}

#[test]
fn a_body_that_is_not_an_object_is_sent_as_generated() {
    let (sent, vars) = body(&doc(), &json!({ "type": "array", "items": { "type": "string" } }));
    assert_eq!(sent, json!([]));
    assert!(vars.is_empty());
}

/// A string schema inside `depth` levels of `anyOf`.
fn nested(depth: usize) -> Value {
    (0..depth).fold(json!({ "type": "string" }), |inner, _| json!({ "anyOf": [inner] }))
}

#[test]
fn alternatives_are_followed_max_hops_deep_and_a_loop_of_them_ends() {
    let doc = doc();
    assert_eq!(types(&doc, &nested(MAX_HOPS)), Some(vec!["string"]));
    assert_eq!(types(&doc, &nested(MAX_HOPS + 1)), None);
    let turn = json!({ "components": { "schemas": {
        "Turn": { "anyOf": [{ "$ref": "#/components/schemas/Turn" }] },
    }}});
    assert_eq!(types(&turn, &json!({ "$ref": "#/components/schemas/Turn" })), None);
}
