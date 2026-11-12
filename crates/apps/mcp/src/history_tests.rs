use serde_json::json;

use super::{ExplainFailure, QueryHistory, is};
use crate::fixture::{args, cx, failed_run, project};
use crate::tools::Tool;

#[test]
fn a_stored_row_belongs_to_its_test_its_example_rows_and_repeats() {
    let test = "users.crud#rejects";
    assert!(is(&json!({"id": "users.crud#rejects"}), test));
    assert!(is(&json!({"id": "users.crud#rejects [row 2]"}), test));
    assert!(is(&json!({"id": "users.crud#rejects › missing @ [repeat 2]"}), test));
    assert!(!is(&json!({"id": "users.crud#rejectsAll"}), test));
    assert!(!is(&json!({"id": "users.crud#reject"}), test));
    assert!(!is(&json!({"name": "users.crud#rejects"}), test));
}

#[tokio::test]
async fn query_history_lists_runs_or_shows_one() {
    let (_dir, engine) = project();
    let cx = cx(&engine, "dev");
    let run = failed_run(&cx).await;
    let runs = QueryHistory.call(&cx, &args(json!({"limit": 5}))).await.unwrap();
    assert_eq!(runs["runs"].as_array().unwrap().len(), 1);
    let one = QueryHistory.call(&cx, &args(json!({"run": run.to_string()}))).await.unwrap();
    assert_eq!(one["tests"][0]["id"], "users.crud#listsUsers");
    assert_eq!(one["tests"][0]["status"], "failed");
}

#[tokio::test]
async fn explain_failure_puts_the_result_beside_the_definition() {
    let (_dir, engine) = project();
    let cx = cx(&engine, "dev");
    failed_run(&cx).await;
    let explained = ExplainFailure.call(&cx, &args(json!({"test": "users.crud#listsUsers"}))).await.unwrap();
    assert_eq!(explained["results"].as_array().unwrap().len(), 1);
    assert_eq!(explained["results"][0]["status"], "failed");
    assert!(explained["results"][0]["message"].is_string(), "{explained}");
    assert!(explained["definition"].as_str().unwrap().contains("/users"), "{explained}");
    assert!(explained["run"].is_object(), "{explained}");
}
