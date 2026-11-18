use std::collections::BTreeSet;

use axum::http::{Method, Request, StatusCode, header};
use saphyr_parser::{Event, Parser, ScalarStyle};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::SPECS;
use crate::router;
use crate::testing::send;

/// The methods the checks try on every path.
const METHODS: [Method; 5] = [Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE];

/// The body each method that takes one sends: a valid one.
const BODIES: [(&str, &str); 3] = [
    ("POST", r#"{"email": "c@example.com"}"#),
    ("PUT", r#"{"email": "d@example.com"}"#),
    ("PATCH", r#"{"role": "member"}"#),
];

/// A container being read, and the key its next value goes under.
type Open = (Value, Option<String>);

/// The YAML `text` as JSON: a plain scalar that reads as JSON (a number, `true`) as that, any
/// other as text.
fn json_of(text: &str) -> Value {
    let (mut open, mut done): (Vec<Open>, Value) = (Vec::new(), Value::Null);
    let mut parser = Parser::new_from_str(text);
    while let Some(next) = parser.next_event() {
        let value = match next.unwrap().0 {
            Event::MappingStart(..) => json!({}),
            Event::SequenceStart(..) => json!([]),
            Event::MappingEnd | Event::SequenceEnd => {
                let (closed, _) = open.pop().unwrap();
                match open.last_mut() {
                    Some(parent) => add(parent, closed),
                    None => done = closed,
                }
                continue;
            }
            Event::Scalar(text, style, ..) => {
                add(open.last_mut().unwrap(), scalar(&text, style));
                continue;
            }
            _ => continue,
        };
        open.push((value, None));
    }
    done
}

fn scalar(text: &str, style: ScalarStyle) -> Value {
    let typed = (style == ScalarStyle::Plain).then(|| serde_json::from_str(text).ok()).flatten();
    typed.unwrap_or_else(|| Value::String(text.to_owned()))
}

fn add((container, key): &mut Open, value: Value) {
    if let Value::Array(items) = container {
        items.push(value);
    } else if let (Value::Object(map), Some(name)) = (&mut *container, key.take()) {
        map.insert(name, value);
    } else {
        *key = Some(value.as_str().map_or_else(|| value.to_string(), str::to_owned));
    }
}

fn spec(name: &str) -> Value {
    json_of(SPECS.iter().find(|(file, _)| *file == name).unwrap().1)
}

/// The `operationId`s of `spec`.
fn operations(spec: &Value) -> BTreeSet<String> {
    let items = spec["paths"].as_object().unwrap().values();
    let ops = items.flat_map(|item| item.as_object().unwrap().values().filter_map(|op| op["operationId"].as_str()));
    ops.map(str::to_owned).collect()
}

/// `method path` on `app` as the checks send it: `{id}` a user made for it, with a valid body.
async fn call(app: &axum::Router, method: &Method, path: &str) -> (StatusCode, Value) {
    let mut path = path.to_owned();
    if path.contains("{id}") {
        let made = send(app, Method::POST, "/users", r#"{"email": "e@example.com"}"#).await.1;
        path = path.replace("{id}", made["id"].as_str().unwrap());
    }
    let body = BODIES.iter().find(|(name, _)| *name == method.as_str()).map_or("", |(_, body)| body);
    send(app, method.clone(), &path, body).await
}

/// Whether `body` holds to `schema`, its `$ref`s read in `spec`.
fn holds(spec: &Value, schema: &Value, body: &Value) -> bool {
    let root = json!({"allOf": [schema], "components": spec["components"]});
    jsonschema::validator_for(&root).unwrap().is_valid(body)
}

#[test]
fn the_specs_are_yaml_read_as_json() {
    let users = spec("users.yaml");
    assert_eq!(users["openapi"], "3.0.3");
    assert_eq!(users["servers"][0]["url"], "http://localhost:7421");
    assert_eq!(users["paths"]["/users"]["get"]["parameters"][0]["schema"]["minimum"], 0);
    assert!(users["paths"]["/users"]["get"]["responses"]["200"].is_object(), "a quoted code stays text");
}

#[test]
fn the_versions_drift_as_vc_sync_shows() {
    let ids = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<BTreeSet<_>>();
    assert_eq!(operations(&spec("users-v1.yaml")), ids(&["createUser", "deleteUser", "getUser", "listUsers"]));
    assert_eq!(operations(&spec("users-v2.yaml")), ids(&["createUser", "getUser", "listUsers", "updateUser"]));
    let current = ["createUser", "deleteUser", "getUser", "listUsers", "replaceUser", "updateUser"];
    assert_eq!(operations(&spec("users.yaml")), ids(&current));
}

#[tokio::test]
async fn every_operation_answers_as_its_spec_says() {
    for (name, _) in SPECS {
        let (spec, app) = (spec(name), router());
        for (path, item) in spec["paths"].as_object().unwrap() {
            for method in &METHODS {
                let Some(op) = item.get(method.as_str().to_ascii_lowercase()) else { continue };
                let (status, body) = call(&app, method, path).await;
                let response = &op["responses"][status.as_str()];
                assert!(response.is_object(), "{name}: {method} {path} answered {status}, which it does not list");
                if let Some(schema) = response.pointer("/content/application~1json/schema") {
                    assert!(holds(&spec, schema, &body), "{name}: {method} {path} answered {body}");
                }
            }
        }
    }
}

#[tokio::test]
async fn the_current_spec_lists_every_users_route() {
    let (spec, app) = (spec("users.yaml"), router());
    let paths = spec["paths"].as_object().unwrap();
    assert_eq!(paths.keys().collect::<Vec<_>>(), ["/users", "/users/{id}"]);
    for (path, item) in paths {
        for method in &METHODS {
            let listed = item.get(method.as_str().to_ascii_lowercase()).is_some();
            let status = call(&app, method, path).await.0;
            assert_eq!(status != StatusCode::METHOD_NOT_ALLOWED, listed, "{method} {path}: {status}");
        }
    }
}

/// `GET path` with `host`, if any: the status, `content-type` and text.
async fn fetched(path: &str, host: Option<&str>) -> (StatusCode, Option<String>, String) {
    let request = Request::builder().uri(path);
    let request = if let Some(host) = host { request.header(header::HOST, host) } else { request };
    let response = router().oneshot(request.body(axum::body::Body::empty()).unwrap()).await.unwrap();
    let kind = response.headers().get(header::CONTENT_TYPE).map(|kind| kind.to_str().unwrap().to_owned());
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, kind, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn a_spec_names_the_server_it_was_asked_on() {
    for (name, text) in SPECS {
        let (status, kind, served) = fetched(&format!("/openapi/{name}"), Some("127.0.0.1:9100")).await;
        assert_eq!((status, kind.as_deref()), (StatusCode::OK, Some("application/yaml")), "{name}");
        assert_eq!(served, text.replace("http://localhost:7421", "http://127.0.0.1:9100"));
        assert_eq!(json_of(&served)["servers"][0]["url"], "http://127.0.0.1:9100");
    }
    let ipv6 = fetched("/openapi/users.yaml", Some("[::1]:7421")).await.2;
    assert!(ipv6.contains("url: http://[::1]:7421\n"), "{ipv6}");
}

#[tokio::test]
async fn a_host_that_is_not_plain_keeps_the_default_server() {
    let long = "h".repeat(256);
    for host in [None, Some("evil.example/#"), Some("a b"), Some("x'y"), Some(long.as_str())] {
        let served = fetched("/openapi/users.yaml", host).await.2;
        assert!(served.contains("url: http://localhost:7421\n"), "{host:?}");
    }
}

#[tokio::test]
async fn only_the_three_specs_are_served() {
    for path in ["/openapi/users-v3.yaml", "/openapi/users", "/openapi/", "/openapi/%FF"] {
        assert_eq!(fetched(path, None).await.0, StatusCode::NOT_FOUND, "{path}");
    }
}
