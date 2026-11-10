use std::fs;

use serde_json::json;
use vc_engine::EngineError;

use super::{CallApi, RunTests, WriteDefinition};
use crate::McpError;
use crate::fixture::{CRUD, args, cx, project};
use crate::tools::Tool;

fn refused(result: Result<serde_json::Value, McpError>) -> String {
    match result {
        Err(McpError::Engine(EngineError::Refused { reason, .. })) => reason,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn run_tests_runs_the_selection_and_returns_each_test() {
    let (_dir, engine) = project();
    let cx = cx(&engine, "dev");
    let ran = RunTests.call(&cx, &args(json!({"tags": [], "grep": "every"}))).await.unwrap();
    assert_eq!(ran["exit"], 1);
    assert_eq!(ran["tests"][0]["id"], "users.crud#listsUsers");
    assert_eq!(ran["tests"][0]["status"], "failed");
    assert!(ran["seed"].is_u64());
}

#[tokio::test]
async fn acting_tools_follow_the_profiles_agent_rules() {
    let (dir, engine) = project();
    let bare = cx(&engine, "bare");
    assert_eq!(refused(RunTests.call(&bare, &args(json!({}))).await), "bare: agents.run is false");
    let call = args(json!({"api": "users#create", "vars": {"name": "Ada"}}));
    assert!(refused(CallApi.call(&cx(&engine, "dev"), &call).await).contains("POST"));
    let write = args(json!({"path": "services/users/crud.test.yaml", "text": CRUD}));
    assert!(refused(WriteDefinition.call(&bare, &write).await).contains("writeDefinitions"));
    assert_eq!(fs::read_to_string(dir.path().join("services/users/crud.test.yaml")).unwrap(), CRUD);
}

#[tokio::test]
async fn call_api_sends_an_allowed_api() {
    let (_dir, engine) = project();
    let called = CallApi.call(&cx(&engine, "dev"), &args(json!({"api": "users#list", "keep": true}))).await.unwrap();
    assert_eq!(called["exit"], 1, "{called}");
    assert!(called["text"].is_string());
}

#[tokio::test]
async fn write_definition_writes_a_file_that_compiles() {
    let (dir, engine) = project();
    let text = "tests:\n  - id: createsUser\n    call: users#create\n";
    let path = "services/users/create.test.yaml";
    let written = WriteDefinition.call(&cx(&engine, "dev"), &args(json!({"path": path, "text": text}))).await.unwrap();
    assert_eq!(written, json!({"written": [path], "titles": [format!("Create {path}")]}));
    assert_eq!(fs::read_to_string(dir.path().join(path)).unwrap(), text);
    let missing = WriteDefinition.call(&cx(&engine, "dev"), &args(json!({"path": path}))).await;
    assert_eq!(missing.unwrap_err().to_string(), "argument `text`: is required");
}
