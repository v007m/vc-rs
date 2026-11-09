use std::fs;

use serde_json::{Value, json};

use super::{Check, GetDefinition, ListDefinitions, Search};
use crate::fixture::{args, cx, project};
use crate::tools::Tool;

fn ids(result: &Value) -> Vec<&str> {
    result["items"].as_array().unwrap().iter().map(|item| item["id"].as_str().unwrap()).collect()
}

#[tokio::test]
async fn list_definitions_lists_tests_by_default_and_apis_or_files_when_asked() {
    let (_dir, engine) = project();
    let cx = cx(&engine, "dev");
    let tests = ListDefinitions.call(&cx, &args(json!({}))).await.unwrap();
    assert_eq!(ids(&tests), ["users.crud#listsUsers"]);
    assert_eq!(tests["items"][0]["name"], "Lists every user");
    let apis = ListDefinitions.call(&cx, &args(json!({"kind": "apis"}))).await.unwrap();
    assert_eq!(ids(&apis), ["users#list", "users#create"]);
    let files = ListDefinitions.call(&cx, &args(json!({"kind": "files"}))).await.unwrap();
    assert_eq!(files["items"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn list_definitions_names_an_unknown_kind() {
    let (_dir, engine) = project();
    let refused = ListDefinitions.call(&cx(&engine, "dev"), &args(json!({"kind": "loads"}))).await;
    assert_eq!(refused.unwrap_err().to_string(), "argument `kind`: `loads` is not tests, apis or files");
}

#[tokio::test]
async fn search_matches_ids_and_names_ignoring_case() {
    let (_dir, engine) = project();
    let cx = cx(&engine, "dev");
    let by_name = Search.call(&cx, &args(json!({"query": "EVERY user"}))).await.unwrap();
    assert_eq!(
        by_name,
        json!({"items": [{"kind": "test", "id": "users.crud#listsUsers", "name": "Lists every user", "path": "services/users/crud.test.yaml"}]})
    );
    let by_id = Search.call(&cx, &args(json!({"query": "#create"}))).await.unwrap();
    assert_eq!(ids(&by_id), ["users#create"]);
    assert_eq!(by_id["items"][0]["kind"], "api");
    let none = Search.call(&cx, &args(json!({"query": "refund"}))).await.unwrap();
    assert_eq!(ids(&none), Vec::<&str>::new());
}

#[tokio::test]
async fn get_definition_explains_a_test_or_api() {
    let (_dir, engine) = project();
    let shown = GetDefinition.call(&cx(&engine, "dev"), &args(json!({"id": "users#create"}))).await.unwrap();
    assert_eq!(shown["id"], "users#create");
    assert!(shown["text"].as_str().unwrap().contains("POST"), "{shown}");
    let unknown = GetDefinition.call(&cx(&engine, "dev"), &args(json!({"id": "users#gone"}))).await;
    assert!(unknown.is_err());
}

#[tokio::test]
async fn check_gives_the_diagnostics_and_their_count() {
    let (dir, engine) = project();
    let cx = cx(&engine, "dev");
    let clean = Check.call(&cx, &args(json!({}))).await.unwrap();
    assert_eq!(clean["summary"], json!({"errors": 0, "warnings": 0, "checked": 3}));
    fs::write(dir.path().join("services/users/crud.test.yaml"), "tests:\n  - id: listsUsers\n    call: users#gone\n")
        .unwrap();
    let broken = Check.call(&cx, &args(json!({}))).await.unwrap();
    assert_eq!(broken["summary"]["errors"], 1);
    assert!(broken["diagnostics"].to_string().contains("users#gone"), "{broken}");
}
