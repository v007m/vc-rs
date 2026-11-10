use std::sync::Arc;

use vc_interfaces::{Approvals as _, HistoryReader as _, Verdict};
use vc_model::{HttpResponse, RenderedRequest, RunStatus};
use vc_testkit::{
    FakeAuth, FakeTransport, FakeVcs, MemoryApprovals, MemoryHistory, MemoryKeychain, MemorySources, RecordingBrowser,
    RecordingTransport, Recordings, status,
};

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
    let (transport, recordings) =
        RecordingTransport::new(Arc::new(FakeTransport::new(|request, call| Ok(answer(request, call)))));
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
    assert!(report.text.contains("✗ status: expected [418], got 200"), "{}", report.text);
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
    assert_eq!(
        (runs[0].status, runs[0].totals.passed, runs[0].start.profiles.clone()),
        (Some(RunStatus::Passed), 1, vec!["dev".to_owned()])
    );
}

#[tokio::test]
async fn an_unknown_api_names_the_closest() {
    let (engine, recordings, _) = project();
    let Err(EngineError::Selection { reason, help }) = engine.call(&call("users#lst")).await else { panic!("refused") };
    assert_eq!((reason.as_str(), help.as_deref()), ("no api `users#lst`", Some("did you mean `users#list`?")));
    assert_eq!(sent(&recordings).len(), 0);
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

/// A project whose `users#me` uses the bearer auth `svc`, whose token is `abc`.
const AUTHED: [(&str, &[u8]); 2] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: svc\n    type: bearer\n    token: abc\n",
    ),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /me\n    auth: svc\n"),
];

/// The `authorization` header `users#me` went out with, from an engine `finish` completes.
async fn authorized(finish: impl FnOnce(crate::EngineBuilder) -> crate::EngineBuilder) -> Option<String> {
    let (transport, recordings) = RecordingTransport::new(Arc::new(FakeTransport::new(|_, _| Ok(status(200)))));
    let engine = finish(builder(FakeVcs::default()))
        .sources(Arc::new(MemorySources::new(&AUTHED)))
        .transport(Arc::new(transport))
        .history(Arc::new(MemoryHistory::new()), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    assert_eq!(engine.call(&call("users#me")).await.unwrap().exit, 0);
    let sent = recordings.take();
    sent[0].request.headers.get("authorization").and_then(|value| value.to_str().ok()).map(str::to_owned)
}

#[tokio::test]
async fn a_call_gets_its_credentials_from_the_standard_auth_types() {
    assert_eq!(authorized(|engine| engine).await.as_deref(), Some("Bearer abc"));
}

#[tokio::test]
async fn the_builder_auth_replaces_the_standard_types() {
    let fake = FakeAuth::new(&["bearer"]);
    let given = fake.clone();
    assert_eq!(authorized(|engine| engine.auth(Arc::new(given))).await.as_deref(), Some("Bearer bearer-1"));
    assert_eq!(fake.calls(), 1);
}

/// A project whose `users#me` signs in with `oauth2AuthorizationCode` at `http://id`.
const SIGNED_IN: [(&str, &[u8]); 2] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\nauth:\n  - id: web\n    type: oauth2AuthorizationCode\n    authorizeUrl: http://id/authorize\n    tokenUrl: http://id/token\n    clientId: cli\n",
    ),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /me\n    auth: web\n"),
];

/// Plays the user in the browser: takes the authorize URL the engine opened and follows its
/// redirect with a code and the URL's `state`.
async fn sign_in(mut urls: tokio::sync::mpsc::UnboundedReceiver<String>) -> String {
    let url = urls.recv().await.unwrap();
    let query = url.split_once('?').unwrap().1.to_owned();
    let field = |key: &str| query.split('&').find_map(|pair| pair.strip_prefix(&format!("{key}="))).unwrap().to_owned();
    let (redirect, state) = (field("redirect_uri"), field("state"));
    let port = redirect.strip_prefix("http%3A%2F%2F127.0.0.1%3A").unwrap().split('%').next().unwrap().to_owned();
    tokio::task::spawn_blocking(move || {
        let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        let target = format!("GET /callback?code=c0de&state={state} HTTP/1.1\r\nhost: localhost\r\n\r\n");
        std::io::Write::write_all(&mut stream, target.as_bytes()).unwrap();
        let mut answer = String::new();
        std::io::Read::read_to_string(&mut stream, &mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    })
    .await
    .unwrap();
    url
}

#[tokio::test]
async fn a_sign_in_opens_the_builder_browser() {
    let (transport, recordings) = RecordingTransport::new(Arc::new(FakeTransport::new(|request, _| {
        let mut response = status(200);
        if request.uri.path() == "/token" {
            response.body = r#"{"access_token":"t0k3n","token_type":"bearer","expires_in":3600}"#.into();
        }
        Ok(response)
    })));
    let (browser, urls) = RecordingBrowser::new();
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&SIGNED_IN)))
        .transport(Arc::new(transport))
        .browser(Arc::new(browser))
        .keychain(Arc::new(MemoryKeychain::new()))
        .history(Arc::new(MemoryHistory::new()), Arc::new(MemoryHistory::new()))
        .build()
        .unwrap();
    let request = call("users#me");
    let (report, url) = tokio::join!(engine.call(&request), sign_in(urls));
    assert_eq!(report.unwrap().exit, 0);
    assert!(url.starts_with("http://id/authorize?"), "{url}");
    let sent = recordings.take();
    let me = sent.iter().find(|record| record.request.uri.path() == "/me").unwrap();
    assert_eq!(me.request.headers.get("authorization").unwrap(), "Bearer t0k3n");
}

const AGENTS: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n    agents: {call: [GET], approve: [POST]}\n  - id: bare\nvars: {baseUrl: 'http://x'}\n",
);

fn agent_project(queue: &MemoryApprovals) -> (Engine, Recordings) {
    let (transport, recordings) =
        RecordingTransport::new(Arc::new(FakeTransport::new(|request, call| Ok(answer(request, call)))));
    let engine = builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[AGENTS, USERS])))
        .transport(Arc::new(transport))
        .history(Arc::new(MemoryHistory::new()), Arc::new(MemoryHistory::new()))
        .approvals(Arc::new(queue.clone()), Arc::new(queue.clone()))
        .build()
        .unwrap();
    (engine, recordings)
}

fn agent_call(api: &str, profile: &str) -> CallRequest {
    CallRequest { agent: Some("claude".into()), profiles: vec![profile.into()], ..call(api) }
}

#[tokio::test]
async fn an_agents_call_the_policy_refuses_sends_nothing() {
    let queue = MemoryApprovals::new();
    let (engine, recordings) = agent_project(&queue);
    let refused = engine.call(&agent_call("users#list", "bare")).await;
    let Err(EngineError::Refused { reason, help }) = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "bare: agents.call and agents.approve leave out GET");
    assert!(help.unwrap().contains("`agents` block"));
    assert_eq!(sent(&recordings), Vec::<String>::new());
    assert_eq!(queue.actions()[0].result, format!("refused: {reason}"));
}

#[tokio::test]
async fn an_agents_allowed_call_is_sent_and_logged() {
    let queue = MemoryApprovals::new();
    let (engine, recordings) = agent_project(&queue);
    assert_eq!(engine.call(&agent_call("users#list", "dev")).await.unwrap().exit, 0);
    assert_eq!(sent(&recordings), ["GET /users"]);
    let logged = queue.actions();
    assert_eq!((logged[0].tool.as_str(), logged[0].result.as_str()), ("call_api", "ok"));
    assert_eq!(logged[0].approved_by, None);
}

#[tokio::test]
async fn an_agents_call_that_asks_is_sent_once_a_person_approves() {
    let queue = MemoryApprovals::answering(Verdict::Approve);
    let (engine, recordings) = agent_project(&queue);
    let request = CallRequest { keep: true, ..agent_call("users#create", "dev") };
    assert_eq!(engine.call(&request).await.unwrap().exit, 0);
    assert_eq!(sent(&recordings), ["POST /users"]);
    assert_eq!(queue.list(false).unwrap()[0].request.summary, "POST users#create");
    assert_eq!(queue.actions()[0].approved_by.as_deref(), Some("tester"));
}

#[tokio::test]
async fn an_agents_call_is_judged_on_its_cleanup_too() {
    let queue = MemoryApprovals::new();
    let (engine, recordings) = agent_project(&queue);
    let refused = engine.call(&agent_call("users#create", "dev")).await;
    let Err(EngineError::Refused { reason, .. }) = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "dev: agents.call and agents.approve leave out DELETE");
    assert_eq!(sent(&recordings), Vec::<String>::new());
}

#[tokio::test]
async fn an_agents_call_that_fails_before_the_gate_is_logged() {
    let queue = MemoryApprovals::new();
    let (engine, recordings) = agent_project(&queue);
    let failed = engine.call(&agent_call("users#nope", "dev")).await;
    let Err(EngineError::Selection { reason, .. }) = failed else { panic!("{failed:?}") };
    assert_eq!(sent(&recordings), Vec::<String>::new());
    let logged = queue.actions();
    assert_eq!((logged[0].tool.as_str(), logged[0].result.clone()), ("call_api", format!("failed: {reason}")));
    assert!(logged[0].arguments.contains("users#nope"), "{}", logged[0].arguments);
}
