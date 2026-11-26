//! An ad hoc call is sent as an api would be, with its templates and auth resolved, and its
//! secrets reach neither its report nor its history (D540–D542).

use std::sync::Arc;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use vc_diagnostics::{Category, Code};
use vc_interfaces::{HistoryError, HistoryReader as _, HistoryWriter, Row, RunEnd, RunFilter, RunId, RunStart};
use vc_model::{Keep, RenderedRequest};
use vc_testkit::{
    FakeTransport, FakeVcs, MemoryEnvironment, MemoryHistory, MemorySources, RecordingTransport, Recordings, status,
};

use super::{API_ID, file, resolved};
use crate::sample::builder;
use crate::{AdhocCall, CallRequest, EngineBuilder, EngineError, FileChange};

/// The value of the secret `API_TOKEN`, the auth `svc`'s token.
const TOKEN: &str = "tok-Kd4R-8wNq-Yb6T";

const CONFIG: &str = concat!(
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n",
    "history: {bodies: all}\nauth:\n  - id: svc\n    type: bearer\n    token: ${secret.API_TOKEN}\n",
);

/// An engine over a project with no api, whose `.env` gives `API_TOKEN`; every request answers
/// 201 with its own body. What it sent, and the engine before its history is set.
fn project() -> (EngineBuilder, Recordings) {
    let files: [(&str, &[u8]); 2] = [("vibranium.config.yaml", CONFIG.as_bytes()), (".env.schema", b"API_TOKEN=\n")];
    let echo = FakeTransport::new(|request: &RenderedRequest, _| {
        let mut answer = status(201);
        answer.body = request.body.clone();
        Ok(answer)
    });
    let (transport, recordings) = RecordingTransport::new(Arc::new(echo));
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&files)))
        .environment(Arc::new(MemoryEnvironment::new(&[("API_TOKEN", TOKEN)])))
        .transport(Arc::new(transport));
    (engine, recordings)
}

/// A `POST` to `${var.baseUrl}/echo` with the auth `svc`, the secret in a header and the body.
fn post() -> AdhocCall {
    AdhocCall {
        method: "POST".into(),
        url: "${var.baseUrl}/echo".into(),
        headers: vec![("x-key".into(), "${secret.API_TOKEN}".into()), ("x-note".into(), "plain".into())],
        query: vec![("q".into(), "0".into()), ("q".into(), "1".into())],
        body: Some(r#"{"token": "${secret.API_TOKEN}", "n": 1}"#.into()),
        auth: Some("svc".into()),
        ..AdhocCall::default()
    }
}

fn adhoc(call: AdhocCall) -> CallRequest {
    CallRequest { adhoc: Some(call), ..CallRequest::default() }
}

/// Everything the engine wrote to the history, as text.
struct Written(mpsc::UnboundedSender<String>);

impl HistoryWriter for Written {
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError> {
        self.0.send(format!("{start:?}")).ok();
        Ok(RunId::new(9))
    }

    fn record(&self, _: RunId, rows: Vec<Row>) -> Result<(), HistoryError> {
        for row in &rows {
            if let Row::Exchange(exchange) = row {
                for body in [&exchange.request_body, &exchange.response_body].into_iter().flatten() {
                    self.0.send(format!("body {}", String::from_utf8_lossy(body))).ok();
                }
            }
        }
        self.0.send(format!("{rows:?}")).ok();
        Ok(())
    }

    fn finish(&self, _: RunId, end: RunEnd) -> Result<(), HistoryError> {
        self.0.send(format!("{end:?}")).ok();
        Ok(())
    }

    fn prune(&self, _: Keep) -> Result<usize, HistoryError> {
        Ok(0)
    }
}

#[test]
fn an_ad_hoc_call_is_an_api_that_sends_no_auth_unless_it_names_one() {
    let call = AdhocCall { url: "https://api.test/users".into(), ..AdhocCall::default() };
    let written = file(&call).unwrap();
    assert_eq!(written.path, "services/vc-adhoc/vc-adhoc.api.yaml");
    assert!(written.after.starts_with("apis:\n  - id: call\n"), "{}", written.after);
    assert!(written.after.contains("\n    auth: none\n"), "{}", written.after);
    assert!(!written.after.contains("type:") && !written.after.contains("method:"), "http and GET are the defaults");
    let stream = AdhocCall { kind: "websocket".into(), auth: Some("svc".into()), ..call };
    let written = file(&stream).unwrap().after;
    assert!(written.contains("\n    type: websocket\n") && written.contains("\n    auth: svc\n"), "{written}");
}

#[test]
fn an_ad_hoc_call_names_no_api_or_draft_of_its_own() {
    let request = adhoc(post());
    assert_eq!(resolved(&request).unwrap().api, API_ID);
    let named = CallRequest { api: "users#list".into(), ..request };
    let Err(EngineError::Setting { origin, .. }) = resolved(&named) else { panic!("refused") };
    assert_eq!(origin, "ad hoc call");
    let plain = CallRequest { api: "users#list".into(), ..CallRequest::default() };
    assert_eq!(resolved(&plain).unwrap().api, "users#list", "any other call is as it is");
}

#[tokio::test]
async fn an_ad_hoc_call_is_sent_with_its_templates_and_auth_resolved() {
    let (engine, recordings) = project();
    let history = Arc::new(MemoryHistory::new());
    let engine = engine.history(history.clone(), history).build().unwrap();
    let report = engine.call(&adhoc(post())).await.unwrap();
    assert_eq!(report.exit, 0, "the default expect, 2xx, holds: {}", report.text);
    let sent = recordings.take();
    let request = &sent[0].request;
    assert_eq!((request.method.as_str(), request.uri.to_string()), ("POST", "http://x/echo?q=1".to_owned()));
    let header = |name| request.headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default();
    assert_eq!((header("authorization"), header("x-key")), (format!("Bearer {TOKEN}").as_str(), TOKEN));
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body, json!({"token": TOKEN, "n": 1}), "a JSON body is sent as JSON");
}

#[tokio::test]
async fn a_text_body_is_sent_as_written() {
    let (engine, recordings) = project();
    let history = Arc::new(MemoryHistory::new());
    let engine = engine.history(history.clone(), history).build().unwrap();
    let call = AdhocCall { body: Some("hi from ${var.baseUrl}".into()), auth: None, ..post() };
    assert_eq!(engine.call(&adhoc(call)).await.unwrap().exit, 0);
    let sent = recordings.take();
    assert_eq!(&sent[0].request.body[..], b"hi from http://x");
    assert!(sent[0].request.headers.get("authorization").is_none(), "no auth named, none sent");
}

#[tokio::test]
async fn an_ad_hoc_calls_secrets_reach_neither_its_report_nor_its_history() {
    let (engine, _) = project();
    let (notes, mut written) = mpsc::unbounded_channel();
    let engine = engine.history(Arc::new(Written(notes)), Arc::new(MemoryHistory::new())).build().unwrap();
    let report = engine.call(&adhoc(post())).await.unwrap();
    let shown = format!("{:?} {:?} {}", report.exchange, report.response, report.text);
    assert!(!shown.contains(TOKEN), "{shown}");
    let mut history = Vec::new();
    while let Ok(note) = written.try_recv() {
        history.push(note);
    }
    let history = history.join("\n");
    assert!(history.contains("adhoc") && history.contains("vc-adhoc#call"), "{history}");
    assert!(history.lines().any(|line| line.starts_with("body ")), "bodies are kept under `bodies: all`: {history}");
    assert!(!history.contains(TOKEN), "{history}");
}

#[tokio::test]
async fn the_history_lists_an_ad_hoc_call_by_its_method_url_and_status() {
    let (engine, _) = project();
    let history = Arc::new(MemoryHistory::new());
    let engine = engine.history(history.clone(), history.clone()).build().unwrap();
    engine.call(&adhoc(post())).await.unwrap();
    let filter = RunFilter { kind: Some("adhoc".into()), ..RunFilter::default() };
    let runs = history.runs(10, &filter).unwrap();
    let call = runs[0].call.clone().unwrap();
    assert_eq!((call.method.as_str(), call.status), ("POST", Some(201)));
    assert!(call.url.starts_with("http://x/echo"), "{}", call.url);
}

#[tokio::test]
async fn a_bad_ad_hoc_call_is_refused_before_anything_is_sent() {
    let (engine, recordings) = project();
    let history = Arc::new(MemoryHistory::new());
    let engine = engine.history(history.clone(), history).build().unwrap();
    let bad = [
        AdhocCall { kind: "grpc".into(), ..post() },
        AdhocCall { kind: "websocket".into(), ..post() },
        AdhocCall { url: "/echo".into(), ..post() },
        AdhocCall { body: Some(r#"{"$file": "../../.env"}"#.into()), ..post() },
    ];
    for call in bad {
        let refused = engine.call(&adhoc(call.clone())).await;
        assert!(matches!(&refused, Err(EngineError::Setting { origin, .. }) if origin == "ad hoc call"), "{call:?}");
    }
    let unknown = engine.call(&adhoc(AdhocCall { auth: Some("nope".into()), ..post() })).await;
    assert!(unknown.is_err(), "an auth the project does not define is an error, as in an api file");
    assert_eq!(recordings.take(), []);
}

#[tokio::test]
async fn an_ad_hoc_call_to_a_full_url_is_sent_there() {
    let (engine, recordings) = project();
    let history = Arc::new(MemoryHistory::new());
    let engine = engine.history(history.clone(), history).build().unwrap();
    let call = AdhocCall { url: "http://127.0.0.1:4010/echo".into(), ..post() };
    assert_eq!(engine.call(&adhoc(call)).await.unwrap().exit, 0);
    let sent = recordings.take();
    assert_eq!(sent[0].request.uri.to_string(), "http://127.0.0.1:4010/echo?q=1");
}

#[test]
fn the_ad_hoc_file_is_held_to_no_naming_or_url_convention() {
    let (engine, _) = project();
    let engine = engine.history(Arc::new(MemoryHistory::new()), Arc::new(MemoryHistory::new())).build().unwrap();
    let conventions = |call: &AdhocCall, path: Option<&str>| {
        let written = file(call).unwrap();
        let change = FileChange { path: path.map_or_else(|| written.path.clone(), str::to_owned), ..written };
        let compiled = engine.compiled_with(&[change]).unwrap();
        let codes = compiled.diagnostics.iter().map(|diagnostic| diagnostic.code);
        codes.filter(|code| code.category() == Category::Convention).collect::<Vec<_>>()
    };
    let full = AdhocCall { url: "http://127.0.0.1:4010/users".into(), auth: None, ..post() };
    assert_eq!(conventions(&full, Some("services/other/other.api.yaml")), [], "a full URL breaks no convention");
    let placeholder = AdhocCall { url: "http://127.0.0.1:4010/users/{id}".into(), ..full };
    assert_eq!(conventions(&placeholder, Some("services/other/other.api.yaml")), [Code::V4006], "in a user's file");
    assert_eq!(conventions(&placeholder, None), [], "not in the file nobody wrote (D545)");
}
