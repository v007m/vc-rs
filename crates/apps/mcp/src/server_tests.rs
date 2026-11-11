use std::path::Path;

use serde_json::{Value, json};
use vc_engine::EngineBuilder;

use super::VcServer;
use crate::fixture::{args, project};

fn server(dir: &Path, profile: &str) -> VcServer {
    let engine = EngineBuilder::for_project(dir).unwrap().build().unwrap();
    VcServer::new(engine, vec![profile.to_owned()], Vec::new())
}

/// An `agent_actions` row: agent, tool, profile, result.
type Logged = (String, String, Option<String>, String);

fn logged(dir: &Path) -> Vec<Logged> {
    let db = rusqlite::Connection::open(dir.join(".vibranium/history.db")).unwrap();
    let mut rows = db.prepare("SELECT agent, tool, profile, result FROM agent_actions ORDER BY id").unwrap();
    let read = |row: &rusqlite::Row<'_>| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?));
    rows.query_map([], read).unwrap().map(Result::unwrap).collect()
}

fn content(result: &rmcp::model::CallToolResult) -> &Value {
    result.structured_content.as_ref().unwrap()
}

#[tokio::test]
async fn a_reading_tool_answers_with_structured_content_and_is_logged() {
    let (dir, _engine) = project();
    let answered = server(dir.path(), "dev").answer("search", "claude", args(json!({"query": "list"}))).await.unwrap();
    assert_eq!(answered.is_error, Some(false));
    assert_eq!(content(&answered)["items"][0]["id"], "users.crud#listsUsers");
    let row = ("claude".to_owned(), "search".to_owned(), Some("dev".to_owned()), "ok".to_owned());
    assert_eq!(logged(dir.path()), [row]);
}

#[tokio::test]
async fn a_reading_tool_that_fails_is_logged_as_failed() {
    let (dir, _engine) = project();
    let answered =
        server(dir.path(), "dev").answer("get_definition", "claude", args(json!({"id": "users#gone"}))).await.unwrap();
    assert_eq!(answered.is_error, Some(true));
    assert_eq!(content(&answered)["kind"], "error");
    let rows = logged(dir.path());
    assert!(rows[0].3.starts_with("failed: "), "{rows:?}");
}

#[tokio::test]
async fn a_refusal_says_why_and_what_to_change() {
    let (dir, _engine) = project();
    let answered = server(dir.path(), "bare").answer("run_tests", "claude", args(json!({}))).await.unwrap();
    assert_eq!(answered.is_error, Some(true));
    let refusal = content(&answered);
    assert_eq!(refusal["kind"], "refused");
    assert!(refusal["reason"].as_str().unwrap().contains("bare: agents.run is false"), "{refusal}");
    assert!(refusal["help"].is_string(), "{refusal}");
    assert_eq!(logged(dir.path())[0].1, "run_tests");
}

#[tokio::test]
async fn a_bad_argument_or_tool_name_is_reported() {
    let (dir, _engine) = project();
    let server = server(dir.path(), "dev");
    let answered = server.answer("search", "claude", args(json!({"query": 3}))).await.unwrap();
    assert_eq!(content(&answered)["kind"], "arguments");
    assert_eq!(content(&answered)["reason"], "argument `query`: must be a string");
    let unknown = server.answer("run", "claude", args(json!({}))).await.unwrap_err();
    assert_eq!(unknown.message, "no tool is called `run`");
}

#[tokio::test]
async fn an_unknown_argument_is_refused_before_the_tool_runs() {
    let (dir, _engine) = project();
    let server = server(dir.path(), "dev");
    let answered = server.answer("run_tests", "claude", args(json!({"select": "users.crud"}))).await.unwrap();
    assert_eq!(answered.is_error, Some(true));
    assert_eq!(content(&answered)["kind"], "arguments");
    let reason = content(&answered)["reason"].as_str().unwrap();
    assert!(reason.starts_with("argument `select`: is not one this tool takes; it takes "), "{reason}");
    let runs = rusqlite::Connection::open(dir.path().join(".vibranium/history.db"))
        .and_then(|db| db.query_row("SELECT count(*) FROM runs", [], |row| row.get::<_, i64>(0)))
        .unwrap_or(0);
    assert_eq!(runs, 0, "nothing runs");
}

#[test]
fn the_server_offers_tools_and_says_what_it_is() {
    let (dir, _engine) = project();
    let info = rmcp::ServerHandler::get_info(&server(dir.path(), "dev"));
    assert_eq!(info.server_info.name, "vibranium");
    assert!(info.capabilities.tools.is_some());
    assert!(info.instructions.is_some_and(|text| text.contains("agent rules")));
}
