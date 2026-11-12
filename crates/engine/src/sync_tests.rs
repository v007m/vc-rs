use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Value, json};
use vc_interfaces::SourceProvider;
use vc_testkit::{FakeTransport, FakeVcs, MemoryCompileCache, MemoryEnvironment, MemorySources, status};

use super::{SyncReport, SyncRequest};
use crate::sync_plan::SyncSide;
use crate::{Engine, EngineBuilder, EngineError, import_write};

const URL: &str = "https://specs.test/users.json";
const CONFIG: &str = "syntaxVersion: 2\nvars: {usersUrl: 'http://x'}\n";

fn api_file(update_method: &str) -> String {
    format!(
        "baseUrl: ${{var.usersUrl}}\nsource:\n  openapi: {URL}\napis:\n  - id: update\n    method: {update_method}\n    \
         path: /users/${{var.userId}}\n    vars:\n      userId: 1\n    operationId: updateUser\n  - id: legacyImport\n    \
         method: post\n    path: /import\n    operationId: legacyImport\n  - id: listUsers\n    path: /users\n    \
         operationId: listUsers\n"
    )
}

fn user_id() -> Value {
    json!([{ "name": "userId", "in": "path", "required": true, "schema": { "type": "integer" } }])
}

fn base() -> Value {
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Users", "version": "1" },
        "paths": {
            "/users": { "get": { "operationId": "listUsers" } },
            "/users/{userId}": { "put": { "operationId": "updateUser", "parameters": user_id() } },
            "/import": { "post": { "operationId": "legacyImport" } }
        }
    })
}

fn theirs() -> Value {
    let cursor = json!([{ "name": "cursor", "in": "query", "required": true, "schema": { "type": "string" } }]);
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Users", "version": "2" },
        "paths": {
            "/users": { "get": { "operationId": "listUsers", "parameters": cursor } },
            "/users/{userId}": { "patch": { "operationId": "updateUser", "parameters": user_id() } },
            "/users/{userId}/restore": { "post": { "operationId": "restoreUser", "parameters": user_id() } }
        }
    })
}

fn engine(api: &str, served: &Value) -> Engine {
    let snapshot = import_write::snapshot("users", URL, &base()).after;
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", CONFIG.as_bytes()),
        ("services/users/users.api.yaml", api.as_bytes()),
        ("openapi/users.yaml", snapshot.as_bytes()),
    ];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    let body = served.to_string();
    let transport = FakeTransport::new(move |request, _| {
        let found = request.uri.path() == "/users.json";
        let mut response = status(if found { 200 } else { 404 });
        response.body = if found { body.clone().into() } else { "".into() };
        Ok(response)
    });
    EngineBuilder::new(PathBuf::from("/project"))
        .sources(sources)
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .vcs(Arc::new(FakeVcs::default()))
        .transport(Arc::new(transport))
        .build()
        .unwrap()
}

fn file<'r>(report: &'r SyncReport, path: &str) -> Option<&'r str> {
    report.changes.files.iter().find(|file| file.path == path).map(|file| file.after.as_str())
}

const API: &str = "services/users/users.api.yaml";
const SNAPSHOT: &str = "openapi/users.yaml";

#[tokio::test]
async fn a_changed_spec_updates_adds_and_deprecates_and_moves_the_snapshot() {
    let report = engine(&api_file("put"), &theirs()).sync(&SyncRequest::default()).await.unwrap();
    assert!(report.drift);
    assert_eq!(report.conflicts, 0);
    let lines = report.changes.titles.join("\n");
    assert!(lines.starts_with(&format!("fetched {URL} (3 operations)")), "{lines}");
    for line in [
        "  ↑ users#update method PUT → PATCH",
        "  ↑ users#listUsers query cursor none → ${var.cursor}",
        "  + users#restoreUser new operation POST /users/${var.userId}/restore",
        "  - users#legacyImport removed from the spec: kept, marked deprecated",
    ] {
        assert!(lines.contains(line), "{line} in\n{lines}");
    }
    let api = file(&report, API).unwrap_or_default();
    assert!(
        api.contains("method: PATCH") && api.contains("deprecated: true") && api.contains("id: restoreUser"),
        "{api}"
    );
    assert!(api.contains("cursor: ${var.cursor}"), "{api}");
    let snapshot = file(&report, SNAPSHOT).unwrap_or_default();
    assert!(snapshot.contains("restoreUser") && !snapshot.contains("legacyImport"), "{snapshot}");
}

#[tokio::test]
async fn a_file_that_took_only_some_operations_is_not_given_the_spec_s_new_ones() {
    let only = "  only:\n    - updateUser\n    - listUsers\n    - legacyImport\napis:";
    let api = api_file("put").replacen("apis:", only, 1);
    let report = engine(&api, &theirs()).sync(&SyncRequest::default()).await.unwrap();
    let lines = report.changes.titles.join("\n");
    assert!(lines.contains("  ↑ users#update method PUT → PATCH"), "{lines}");
    assert!(!lines.contains("restoreUser"), "{lines}");
    let api = file(&report, API).unwrap_or_default();
    assert!(api.contains("method: PATCH") && !api.contains("id: restoreUser"), "{api}");
}

#[tokio::test]
async fn a_new_optional_query_param_is_listed_and_any_other_drift_says_so() {
    let mut optional = base();
    optional["paths"]["/users"]["get"]["parameters"] =
        json!([{ "name": "limit", "in": "query", "schema": { "type": "integer" } }]);
    let report = engine(&api_file("put"), &optional).sync(&SyncRequest::default()).await.unwrap();
    let lines = report.changes.titles.join("\n");
    let added = "  ~ users#listUsers optional query limit added to the spec: not sent, add `query.limit` to send it";
    assert!(report.drift && lines.contains(added), "{lines}");
    assert!(file(&report, API).is_none() && file(&report, SNAPSHOT).is_some(), "{:?}", report.changes.files);
    let mut described = base();
    described["info"]["description"] = json!("Now with a description");
    let report = engine(&api_file("put"), &described).sync(&SyncRequest::default()).await.unwrap();
    let outside = "  ~ users the spec changed outside what sync takes into the file: only the snapshot is updated";
    assert!(report.drift && report.changes.titles.iter().any(|line| line == outside), "{:?}", report.changes.titles);
}

#[tokio::test]
async fn a_conflict_leaves_the_api_and_its_snapshot_entry_until_settled() {
    let engine = engine(&api_file("post"), &theirs());
    let report = engine.sync(&SyncRequest { files: vec!["users".to_owned()], ..SyncRequest::default() }).await.unwrap();
    assert_eq!(report.conflicts, 1);
    assert!(file(&report, API).is_some_and(|api| api.contains("method: post")));
    let snapshot = file(&report, SNAPSHOT).unwrap_or_default();
    assert!(snapshot.contains("put:") && !snapshot.contains("patch:"), "{snapshot}");
    let settle = vec![("users#update".to_owned(), SyncSide::Spec)];
    let taken = engine.sync(&SyncRequest { settle, ..SyncRequest::default() }).await.unwrap();
    assert_eq!(taken.conflicts, 0);
    assert!(file(&taken, API).is_some_and(|api| api.contains("method: PATCH")));
}

#[tokio::test]
async fn an_unchanged_spec_is_no_drift_and_no_change() {
    let report = engine(&api_file("put"), &base()).sync(&SyncRequest::default()).await.unwrap();
    assert!(!report.drift, "{:?}", report.changes.titles);
    assert!(report.changes.files.is_empty(), "{:?}", report.changes.files);
}

#[tokio::test]
async fn with_no_files_named_only_the_files_with_a_source_are_synced() {
    let unsourced = engine("apis:\n  - id: list\n    name: 'source: none'\n    path: /users\n", &theirs());
    let report = unsourced.sync(&SyncRequest::default()).await.unwrap();
    assert!(report.changes.files.is_empty() && !report.drift, "{report:?}");
}

#[tokio::test]
async fn what_cannot_be_synced_is_refused_with_help() {
    let help = |result: Result<SyncReport, EngineError>| result.err().and_then(|error| error.help());
    let misspelt = SyncRequest { files: vec!["user".to_owned()], ..SyncRequest::default() };
    let unknown = engine(&api_file("put"), &theirs()).sync(&misspelt).await;
    assert_eq!(help(unknown).as_deref(), Some("did you mean `users`?"));
    for path in ["services/users/users.api.yaml", "./services/users/users.api.yaml"] {
        let by_path = SyncRequest { files: vec![path.to_owned()], ..SyncRequest::default() };
        let refused = engine(&api_file("put"), &theirs()).sync(&by_path).await;
        assert_eq!(help(refused).as_deref(), Some("did you mean `vc sync users`?"), "{path}");
    }
    let unsourced = engine("apis:\n  - id: list\n    path: /users\n", &theirs());
    let refused = unsourced.sync(&SyncRequest { files: vec!["users".to_owned()], ..SyncRequest::default() }).await;
    assert_eq!(help(refused).as_deref(), Some("add one, or import the spec with `vc import openapi`"));
    let gone =
        engine(&api_file("put").replace("users.json", "gone.json"), &theirs()).sync(&SyncRequest::default()).await;
    assert_eq!(help(gone).as_deref(), Some(super::FETCH_HELP));
    let settle = vec![("users#updat".to_owned(), SyncSide::Mine)];
    let bad = engine(&api_file("put"), &theirs()).sync(&SyncRequest { settle, ..SyncRequest::default() }).await;
    assert_eq!(help(bad).as_deref(), Some("did you mean `users#update`?"));
}

#[tokio::test]
async fn an_api_file_that_does_not_parse_is_refused_whether_named_or_not() {
    let broken = engine(&api_file("put").replacen("apis:\n", "apis: [\n", 1), &theirs());
    for files in [vec![], vec!["users".to_owned()]] {
        let error = broken.sync(&SyncRequest { files: files.clone(), ..SyncRequest::default() }).await.unwrap_err();
        assert!(error.to_string().starts_with(&format!("{API} does not parse: ")), "{files:?}: {error}");
        assert_eq!(error.help().as_deref(), Some("fix it first; `vc check` shows where"), "{files:?}");
    }
}
