use std::sync::Arc;
use std::time::Duration as StdDuration;

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_model::{ActionId, Check, CheckKind, Expect, HttpResponse, Matcher, Origin, OwnerKey, Poll, TemplateValue};
use vc_testkit::FakeTransport;

use super::Schemas;
use crate::fixture::{self, Fixture};
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PipelineParts};

/// The v1 shop's user schema, a draft-06 file.
const USER: &str = r#"{
    "$schema": "http://json-schema.org/draft-06/schema",
    "type": "object",
    "required": ["id", "name", "email", "isAdmin"],
    "properties": {
        "id": { "type": "string", "minLength": 32, "maxLength": 32 },
        "name": { "type": "string" },
        "email": { "type": "string" },
        "role": { "type": "string", "enum": ["ADMIN", "MEMBER"] },
        "isAdmin": { "type": "boolean" }
    }
}"#;

fn user() -> Value {
    serde_json::from_str(USER).unwrap()
}

fn valid_user() -> Value {
    json!({"id": "a".repeat(32), "name": "Ada", "email": "ada@shop.test", "isAdmin": false})
}

fn check(kind: CheckKind, action: ActionId, key: &str) -> Check {
    let matcher = Matcher::Equals(TemplateValue::String(fixture::literal(key)));
    Check { kind, target: None, matcher, origin: Origin::Action(action), span: fixture::span() }
}

/// A fixture whose one call checks its body against the schema each of `keys` names, holding
/// `schemas`.
fn checking(keys: &[&str], schemas: &[(&str, Value)]) -> (Fixture, ActionId) {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users/1");
    fixture.action_mut(call).expect.checks = keys.iter().map(|key| check(CheckKind::Schema, call, key)).collect();
    fixture.program.schemas = schemas.iter().map(|(key, schema)| ((*key).to_owned(), schema.clone())).collect();
    (fixture, call)
}

fn compiled(keys: &[&str], schemas: &[(&str, Value)]) -> Schemas {
    Schemas::compile(&checking(keys, schemas).0.program)
}

#[test]
fn a_draft_06_file_is_judged_by_its_draft_and_each_break_is_reported_by_path() {
    let schemas = compiled(&["schemas/user.json"], &[("schemas/user.json", user())]);
    assert_eq!(schemas.check("schemas/user.json", &valid_user()), Ok(vec![]));
    let body = json!({"id": "abc", "name": 1, "email": "ada@shop.test", "isAdmin": false});
    let mut broken = schemas.check("schemas/user.json", &body).unwrap();
    broken.sort();
    assert_eq!(broken.len(), 2, "{broken:?}");
    assert!(broken[0].starts_with("/id: ") && broken[1].starts_with("/name: "), "{broken:?}");
    assert!(!broken.join(" ").contains("abc"), "the body's values are left out: {broken:?}");
    let missing = schemas.check("schemas/user.json", &json!({})).unwrap();
    assert!(missing.iter().all(|reason| reason.starts_with("/: ")), "{missing:?}");
}

#[test]
fn the_draft_is_the_schemas_own_and_2020_12_when_it_names_none() {
    let schema = |draft: Option<&str>| {
        let mut schema =
            json!({"definitions": {"text": {"type": "string"}}, "$ref": "#/definitions/text", "minLength": 5});
        if let Some(draft) = draft {
            schema["$schema"] = json!(draft);
        }
        schema
    };
    let draft_06 = ("schemas/old.json", schema(Some("http://json-schema.org/draft-06/schema#")));
    let schemas = compiled(&["schemas/old.json", "schemas/new.json"], &[draft_06, ("schemas/new.json", schema(None))]);
    assert_eq!(schemas.check("schemas/old.json", &json!("abc")), Ok(vec![]), "draft-06 ignores $ref's siblings");
    assert_eq!(schemas.check("schemas/new.json", &json!("abc")).map(|broken| broken.len()), Ok(1));
}

#[test]
fn a_pointer_into_a_file_keeps_its_draft_and_its_own_refs() {
    let document = json!({
        "$schema": "http://json-schema.org/draft-04/schema#",
        "definitions": {
            "positive": {"type": "number", "minimum": 0, "exclusiveMinimum": true},
            "user": {"type": "object", "properties": {"age": {"$ref": "#/definitions/positive"}}},
        },
    });
    let key = "schemas/user.json#/definitions/user";
    let schemas = compiled(&[key], &[("schemas/user.json", document)]);
    assert_eq!(schemas.check(key, &json!({"age": 1})), Ok(vec![]));
    assert_eq!(schemas.check(key, &json!({"age": 0})).map(|broken| broken.len()), Ok(1));
}

#[test]
fn a_schema_not_loaded_or_not_valid_fails_with_why() {
    let inline = "services/users/crud.test.yaml:9:11";
    let keys = ["schemas/gone.json", "schemas/gone.json#/x", inline, "schemas/user.json#/nowhere"];
    let schemas = compiled(&keys, &[(inline, json!({"type": 12})), ("schemas/user.json", user())]);
    let body = json!({});
    let unloaded = |key: &str| Err(format!("no schema is loaded for `{key}`"));
    assert_eq!(schemas.check("schemas/gone.json", &body), unloaded("schemas/gone.json"));
    assert_eq!(schemas.check("schemas/gone.json#/x", &body), unloaded("schemas/gone.json#/x"));
    assert_eq!(schemas.check("never/checked.json", &body), unloaded("never/checked.json"));
    let invalid = schemas.check(inline, &body).unwrap_err();
    assert!(invalid.starts_with(&format!("the schema `{inline}` is not a valid JSON Schema: ")), "{invalid}");
    let nowhere = schemas.check("schemas/user.json#/nowhere", &body).unwrap_err();
    assert!(nowhere.starts_with("the schema `schemas/user.json#/nowhere` is not a valid JSON Schema: "), "{nowhere}");
}

#[test]
fn only_schema_checks_in_calls_and_polls_are_compiled() {
    let (mut fixture, call) = checking(&["schemas/user.json"], &[("schemas/user.json", user())]);
    let secs = |secs| vc_model::Duration::new(StdDuration::from_secs(secs));
    let until = Expect { checks: vec![check(CheckKind::Schema, call, "schemas/list.json")], replace: false };
    fixture.action_mut(call).poll = Some(Poll { until, every: secs(1), timeout: secs(3) });
    fixture.action_mut(call).expect.checks.push(check(CheckKind::Equals, call, "schemas/other.json"));
    fixture.action_mut(call).expect.checks.push(check(CheckKind::Schema, call, "schemas/user.json"));
    let schemas = Schemas::compile(&fixture.program);
    assert_eq!(format!("{schemas:?}"), r#"{"schemas/list.json", "schemas/user.json"}"#);
}

/// Runs `call` once against a transport answering `body` with a 200.
async fn send(fixture: Fixture, call: ActionId, body: &Value) -> ActionReport {
    let bytes = Bytes::from(body.to_string());
    let headers = HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_static("application/json"))]);
    let response = HttpResponse { status: StatusCode::OK, headers, wire_size: bytes.len() as u64, body: bytes };
    let transport = FakeTransport::scripted(vec![Ok(response)]);
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

#[tokio::test]
async fn a_call_whose_body_breaks_its_schema_fails_with_v5101() {
    let (fixture, call) = checking(&["schemas/user.json"], &[("schemas/user.json", user())]);
    let report = send(fixture, call, &valid_user()).await;
    assert_eq!(report.outcome.failure(), None);
    let (fixture, call) = checking(&["schemas/user.json"], &[("schemas/user.json", user())]);
    let report = send(fixture, call, &json!({"id": 7, "name": 1, "email": 2, "isAdmin": 3, "role": 4, "x": 5})).await;
    let failure = report.outcome.failure().cloned().unwrap_or_else(|| unreachable!("the body breaks the schema"));
    assert_eq!(failure.code, Some(Code::V5101.to_string()));
    assert!(
        failure.message.starts_with("schema: /") && failure.message.ends_with("(and 1 more)"),
        "{}",
        failure.message
    );
    let schema = &report.checks[1];
    assert_eq!((schema.kind, schema.expected.clone()), (CheckKind::Schema, json!("schemas/user.json")));
}
