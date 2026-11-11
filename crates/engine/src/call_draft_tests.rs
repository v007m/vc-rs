use std::fs;
use std::sync::Arc;

use serde_json::{Value, json};
use vc_adapter_system::FsSources;
use vc_executor::MASKED;
use vc_model::{HttpResponse, RenderedRequest};
use vc_testkit::{FakeTransport, FakeVcs, RecordingTransport, Recordings, status};

use super::CallDraft;
use crate::sample::builder;
use crate::{CallRequest, EditOp, Engine, EngineError, PointerEdit};

const CONFIG: &str = "syntaxVersion: 2\nvars:\n  baseUrl: http://x\n";
const API: &str = "services/users/users.api.yaml";
const USERS: &str = "\
baseUrl: ${var.baseUrl}
apis:
  - id: list
    path: /users
  # Reads one user.
  - id: get
    name: Get a user
    path: /users/${var.userId}
    vars: {userId: 1}
    headers:
      X-Key: tok-draft-5c1
    expect: {status: 200}
";

fn answer(request: &RenderedRequest) -> HttpResponse {
    let mut response = status(if request.method == "POST" { 201 } else { 200 });
    response.body = r#"{"id": 7}"#.into();
    response
}

fn project() -> (tempfile::TempDir, Engine, Recordings) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    for (path, text) in [
        ("vibranium.config.yaml", CONFIG),
        (API, USERS),
        (".env", "SHOP_KEY=tok-draft-5c1\n"),
        (".env.schema", "SHOP_KEY=\n"),
    ] {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let (transport, recordings) =
        RecordingTransport::new(Arc::new(FakeTransport::new(|request, _| Ok(answer(request)))));
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let engine = builder(FakeVcs::default()).sources(folder.clone()).writer(folder).transport(Arc::new(transport));
    (dir, engine.build().unwrap(), recordings)
}

fn edit(pointer: &str, op: EditOp, value: &str) -> PointerEdit {
    PointerEdit { pointer: pointer.into(), op, value: value.into() }
}

fn drafted(engine: &Engine, path: &str, edits: Vec<PointerEdit>) -> CallRequest {
    let digest = engine.read_file(API).unwrap().digest;
    let draft = CallDraft { path: path.into(), digest, edits };
    CallRequest { api: "users#get".into(), draft: Some(draft), ..CallRequest::default() }
}

fn sent(recordings: &Recordings) -> Vec<String> {
    let sent = recordings.take();
    sent.iter().map(|record| format!("{} {}", record.request.method, record.request.uri.path())).collect()
}

#[tokio::test]
async fn gives_the_api_as_its_file_writes_it_with_its_place_and_the_secret_masked() {
    let (_dir, engine, _) = project();
    let written = engine.written_api("users#get").await.unwrap();
    assert_eq!((written.path.as_str(), written.pointer.as_str()), (API, "/apis/1"));
    assert_eq!(written.digest, engine.read_file(API).unwrap().digest);
    let api: Value = serde_json::from_str(&written.json).unwrap();
    assert_eq!(
        api,
        json!({
            "id": "get",
            "name": "Get a user",
            "path": "/users/${var.userId}",
            "vars": {"userId": 1},
            "headers": {"X-Key": MASKED},
            "expect": {"status": 200},
        })
    );
    assert!(!format!("{written:?}").contains("tok-draft-5c1"));
    let unknown = engine.written_api("users#gte").await.unwrap_err();
    assert!(matches!(unknown, EngineError::Selection { .. }), "{unknown:?}");
}

#[tokio::test]
async fn a_call_with_a_draft_sends_the_api_as_edited_and_leaves_its_file_as_it_is() {
    let (dir, engine, recordings) = project();
    let edits = vec![
        edit("/apis/1/method", EditOp::Set, "\"POST\""),
        edit("/apis/1/path", EditOp::Set, "\"/users/${var.userId}/copy\""),
        edit("/apis/1/expect/status", EditOp::Set, "201"),
    ];
    let report = engine.call(&drafted(&engine, API, edits)).await.unwrap();
    assert_eq!(report.exit, 0, "{}", report.text);
    assert_eq!(sent(&recordings), ["POST /users/1/copy"]);
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), USERS);
    engine.call(&CallRequest { api: "users#get".into(), ..CallRequest::default() }).await.unwrap();
    assert_eq!(sent(&recordings), ["GET /users/1"], "the next call without a draft sends the file");
}

#[tokio::test]
async fn a_draft_over_a_file_changed_since_it_was_opened_sends_nothing() {
    let (dir, engine, recordings) = project();
    let request = drafted(&engine, API, vec![edit("/apis/1/method", EditOp::Set, "\"POST\"")]);
    fs::write(dir.path().join(API), USERS.replace("/users/${var.userId}", "/people/${var.userId}")).unwrap();
    let stale = engine.call(&request).await.unwrap_err();
    assert!(matches!(stale, EngineError::Stale { .. }), "{stale:?}");
    assert!(sent(&recordings).is_empty());
}

#[tokio::test]
async fn a_draft_that_breaks_the_api_is_its_diagnostics_and_sends_nothing() {
    let (_dir, engine, recordings) = project();
    let request = drafted(&engine, API, vec![edit("/apis/1/method", EditOp::Set, "\"FETCH\"")]);
    let invalid = engine.call(&request).await.unwrap_err();
    assert!(matches!(invalid, EngineError::Invalid { .. }), "{invalid:?}");
    let refused = engine.call(&drafted(&engine, API, vec![edit("/apis/9", EditOp::Remove, "")])).await.unwrap_err();
    assert!(refused.to_string().contains("edit 1 points at nothing"), "{refused}");
    assert!(sent(&recordings).is_empty());
}

#[tokio::test]
async fn a_draft_never_reads_or_compiles_a_file_the_editor_does_not_open() {
    let (dir, engine, recordings) = project();
    fs::create_dir_all(dir.path().join(".vibranium")).unwrap();
    fs::write(dir.path().join(".vibranium/x.api.yaml"), USERS).unwrap();
    for path in [".env", ".git/config", ".vibranium/x.api.yaml", "../users.api.yaml", "/etc/hosts"] {
        let refused = engine.call(&drafted(&engine, path, Vec::new())).await.unwrap_err();
        assert!(matches!(refused, EngineError::Edit { .. }), "{path}: {refused:?}");
        assert!(!format!("{refused} {refused:?}").contains("tok-draft-5c1"), "{path}: {refused}");
    }
    assert!(sent(&recordings).is_empty());
}
