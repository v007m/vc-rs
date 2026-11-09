use std::sync::Arc;

use vc_interfaces::HistoryReader as _;
use vc_model::{HttpResponse, RenderedRequest, RunStatus};
use vc_testkit::{FakeTransport, FakeVcs, MemoryHistory, MemorySources, RecordingTransport, Recordings, status};

use crate::sample::builder;
use crate::{CallRequest, Engine, EngineError};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nbeforeRun:\n  - call: users#create\n",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n  - id: teapot\n    path: /users\n    expect: {status: 418}\n  - id: create\n    method: POST\n    path: /users\n    expect: {status: 201}\n    cleanup: {call: remove, vars: {userId: '${response.body.id}'}}\n  - id: remove\n    method: DELETE\n    path: /users/${var.userId}\n    vars: {userId: 0}\n  - id: send\n    method: POST\n    path: /echo/${var.word}\n    vars: {word: none, token: 'Bearer abc'}\n",
);

/// `POST` answers 201 with `{"id": 7}`, every other request 200 with `{"token": "s3cr3t"}`.
fn answer(request: &RenderedRequest, _: usize) -> HttpResponse {
    let mut response = status(if request.method == "POST" { 201 } else { 200 });
    response.body = if request.method == "POST" { r#"{"id": 7}"#.into() } else { r#"{"token": "s3cr3t"}"#.into() };
    response
}

fn project() -> (Engine, Recordings, Arc<MemoryHistory>) {
    let (transport, recordings) = RecordingTransport::new(Arc::new(FakeTransport::new(|request, call| Ok(answer(request, call)))));
    let history = Arc::new(MemoryHistory::new());
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[CONFIG, USERS])))
        .transport(Arc::new(transport))
        .history(history.clone(), history.clone())
        .build()
        .unwrap();
    (engine, recordings, history)
}

fn sent(recordings: &Recordings) -> Vec<String> {
    recordings.take().iter().map(|record| format!("{} {}", record.request.method, record.request.uri.path())).collect()
}

fn call(api: &str) -> CallRequest {
    CallRequest { api: api.into(), ..CallRequest::default() }
}

#[tokio::test]
async fn sends_the_api_alone_and_runs_its_cleanup_afterwards() {
    let (engine, recordings, _) = project();
    let report = engine.call(&call("users#create")).await.unwrap();
    assert_eq!(report.exit, 0);
    assert_eq!(report.text, "201 Created · 0 ms · api expect passed\n{\n  \"id\": 7\n}\n");
    assert_eq!(report.response.as_ref().map(|response| response["body"]["id"].clone()), Some(7.into()));
    assert_eq!(sent(&recordings), ["POST /users", "DELETE /users/7"], "beforeRun is skipped");
}

#[tokio::test]
async fn keep_leaves_the_cleanup_out() {
    let (engine, recordings, _) = project();
    let report = engine.call(&CallRequest { keep: true, ..call("users#create") }).await.unwrap();
    assert_eq!((report.exit, sent(&recordings)), (0, vec!["POST /users".to_owned()]));
}

#[tokio::test]
async fn a_var_given_beats_the_api_var() {
    let (engine, recordings, _) = project();
    let request = CallRequest { vars: vec!["word=hello".into(), "x=a=b".into()], ..call("users#send") };
    assert_eq!(engine.call(&request).await.unwrap().exit, 0);
    assert_eq!(sent(&recordings), ["POST /echo/hello"]);
}

#[tokio::test]
async fn a_failed_expect_exits_1_and_says_why() {
    let (engine, _, history) = project();
    let report = engine.call(&call("users#teapot")).await.unwrap();
    assert_eq!(report.exit, 1);
    assert!(report.text.starts_with("200 OK · 0 ms · api expect failed\n"), "{}", report.text);
    assert!(report.text.contains("✗ status: expected 418"), "{}", report.text);
    let runs = history.runs(10).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!((runs[0].start.kind.as_str(), runs[0].status), ("adhoc", Some(RunStatus::Failed)));
    assert_eq!((runs[0].start.selector.as_str(), runs[0].totals.failed), ("users#teapot", 1));
}

#[tokio::test]
async fn a_passed_call_is_recorded_as_passed() {
    let (engine, _, history) = project();
    engine.call(&call("users#list")).await.unwrap();
    let runs = history.runs(10).unwrap();
    assert_eq!((runs[0].status, runs[0].totals.passed, runs[0].start.profiles.clone()), (Some(RunStatus::Passed), 1, vec!["dev".to_owned()]));
}

#[tokio::test]
async fn an_unknown_api_names_the_closest() {
    let (engine, recordings, _) = project();
    let Err(EngineError::Selection { reason, help }) = engine.call(&call("users#lst")).await else { panic!("refused") };
    assert_eq!((reason.as_str(), help.as_deref()), ("no api `users#lst`", Some("did you mean `users#list`?")));
    assert!(sent(&recordings).is_empty());
}

#[tokio::test]
async fn a_var_without_a_name_is_refused() {
    let (engine, _, _) = project();
    for var in ["word", "=hello"] {
        let request = CallRequest { vars: vec![var.into()], ..call("users#send") };
        let Err(EngineError::Setting { origin, .. }) = engine.call(&request).await else { panic!("{var} refused") };
        assert_eq!(origin, "--var");
    }
}
