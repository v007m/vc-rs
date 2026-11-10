use std::sync::Arc;

use http::HeaderValue;
use serde_json::{Value, json};
use vc_executor::MASKED;
use vc_interfaces::TransportError;
use vc_model::{CheckKind, HttpResponse, RenderedRequest};
use vc_testkit::{FakeTransport, FakeVcs, MemorySources, status};

use crate::sample::builder;
use crate::{CallReport, CallRequest, Engine};

/// The session token the login answers with: test data, in no real provider's key format.
const TOKEN: &str = "sess-fixture-91d4";

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nredact:\n  fields: [token]\n  headers: [session]\n",
);
const AUTH: (&str, &[u8]) = (
    "services/auth/auth.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: login\n    method: POST\n    path: /login\n    body: {user: ada}\n    expect: {status: 201, body: {match: {id: 7}}}\n    save:\n      userId: ${response.body.id}\n      sessionToken: ${response.body.token}\n      sessionHeader: ${response.headers.session}\n  - id: down\n    path: /down\n",
);

/// `/login` answers 201 with an id and the session token, in the body and a header; `/down` cannot be reached.
fn answer(request: &RenderedRequest, _: usize) -> Result<HttpResponse, TransportError> {
    if request.uri.path() == "/down" {
        return Err(TransportError::Connect { host: "x:80".into(), reason: "refused".into() });
    }
    let mut response = status(201);
    response.headers.insert("session", HeaderValue::from_static(TOKEN));
    response.body = format!(r#"{{"id": 7, "token": "{TOKEN}"}}"#).into();
    Ok(response)
}

fn project() -> Engine {
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[CONFIG, AUTH])))
        .transport(Arc::new(FakeTransport::new(answer)))
        .build()
        .unwrap()
}

async fn call(api: &str) -> CallReport {
    project().call(&CallRequest { api: api.into(), ..CallRequest::default() }).await.unwrap()
}

#[tokio::test]
async fn a_call_reports_its_exchange_and_checks_masked() {
    let report = call("auth#login").await;
    let exchange = report.exchange.unwrap();
    assert_eq!((exchange.request.method.as_str(), exchange.response.status.as_u16()), ("POST", 201));
    assert_eq!(exchange.request.body.as_ref(), br#"{"user":"ada"}"#);
    let body: Value = serde_json::from_slice(&exchange.response.body).unwrap();
    assert_eq!(body, json!({"id": 7, "token": MASKED}));
    let kinds: Vec<(CheckKind, bool)> = report.checks.iter().map(|check| (check.kind, check.passed)).collect();
    assert_eq!(kinds.len(), 2, "{kinds:?}");
    assert!(kinds.iter().all(|(_, passed)| *passed), "{kinds:?}");
    assert_eq!(kinds[0].0, CheckKind::Status);
}

#[tokio::test]
async fn a_saved_value_names_its_source_and_one_read_from_a_masked_field_is_masked_whole() {
    let report = call("auth#login").await;
    let saved = |name: &str| report.saved.iter().find(|saved| saved.name == name).cloned().unwrap();
    let id = saved("userId");
    assert_eq!((id.from.as_str(), id.value), ("${response.body.id}", json!(7)));
    let token = saved("sessionToken");
    assert_eq!((token.from.as_str(), token.value), ("${response.body.token}", json!(MASKED)));
    let header = saved("sessionHeader");
    assert_eq!((header.from.as_str(), header.value), ("${response.headers.session}", json!(MASKED)));
    assert_eq!(report.response.as_ref().map(|response| response["headers"]["session"].clone()), Some(json!(MASKED)));
    assert!(!format!("{report:?}").contains(TOKEN), "the token leaks: {report:?}");
}

#[tokio::test]
async fn a_call_that_got_no_response_has_no_exchange_and_says_why() {
    let report = call("auth#down").await;
    assert_eq!(report.exit, 1);
    assert!(report.exchange.is_none() && report.saved.is_empty());
    assert!(report.text.contains("cannot connect"), "{}", report.text);
}

const RULED: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nredact:\n  rules:\n    - regex: 'note-[0-9]+'\n    - cel: \"item.name == 'team'\"\n",
);
const NOTES: (&str, &[u8]) = (
    "services/notes/notes.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /note\n    expect: {body: {match: {note: other}}}\n",
);

#[tokio::test]
async fn the_text_response_and_exchange_of_a_call_are_masked_by_the_redact_rules() {
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[RULED, NOTES])))
        .transport(Arc::new(FakeTransport::new(|_, _| {
            let mut response = status(200);
            response.body = r#"{"note": "note-4417", "team": "kestrel"}"#.into();
            Ok(response)
        })))
        .build()
        .unwrap();
    let report = engine.call(&CallRequest { api: "notes#get".into(), ..CallRequest::default() }).await.unwrap();
    assert_eq!(report.exit, 1, "the check on the note failed");
    let shown = format!("{}{:?}{:?}{:?}", report.text, report.response, report.exchange, report.checks);
    assert!(report.text.contains(MASKED), "{}", report.text);
    for value in ["note-4417", "kestrel"] {
        assert!(!shown.contains(value), "{value} shows: {shown}");
    }
}
