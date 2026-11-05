//! `vc call` sends one api ad hoc with the active profile and auth (D130, D178): it runs the
//! api's cleanup unless `--keep`, records the call in history as `adhoc`, skips `beforeRun` and
//! `beforeAll`, and with `--save-as-test` writes a test that expects the status it got (D176).

use serde_json::Value;
use vc_mock_server::MockServer;

use crate::{CONFIG, CRUD, Project, USERS, checks_clean, said};

/// `echo#send`, posting `{word: ${var.word}}` to the mock's `/echo`, which answers
/// `{method, path, query, headers, body}`.
const ECHO: (&str, &str) = (
    "services/echo/echo.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n    vars: {word: none}\n    body: {word: '${var.word}'}\n",
);

async fn served(files: &[(&str, &str)]) -> (MockServer, Project) {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(files);
    project.serve_from(&server.base_url());
    (server, project)
}

#[tokio::test(flavor = "multi_thread")]
async fn calls_one_api_and_prints_the_response() {
    let (_server, project) = served(&[CONFIG, USERS]).await;
    let output = project.vc(&["call", "users#list"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("200 OK · "), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn vars_on_the_command_line_reach_the_request() {
    let (_server, project) = served(&[CONFIG, ECHO]).await;
    let output = project.vc(&["call", "echo#send", "--var", "word=hello"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("hello"), "the body is printed: {}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_the_cleanup_afterwards_unless_kept() {
    let (server, project) = served(&[CONFIG, USERS]).await;
    project.vc(&["call", "users#create"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("POST /users"), stats.count("DELETE /users/{id}")), (1, 1), "{stats:?}");
    project.vc(&["call", "users#create", "--keep"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("POST /users"), stats.count("DELETE /users/{id}")), (2, 1), "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn skips_before_run_and_before_all() {
    let config = format!("{}beforeRun:\n  - call: users#create\n", CONFIG.1);
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[(CONFIG.0, &config.replace("http://x", &server.base_url())), USERS]);
    project.vc(&["call", "users#list"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("GET /users"), stats.count("POST /users")), (1, 0), "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn records_the_call_in_history_as_adhoc() {
    let (_server, project) = served(&[CONFIG, USERS]).await;
    project.vc(&["call", "users#list"]).assert().code(0);
    let output = project.vc(&["history", "--json"]).output().unwrap();
    let runs: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(runs.as_array().map(Vec::len), Some(1), "{runs:#}");
    assert_eq!(runs[0]["kind"], "adhoc", "{runs:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_api_expect_exits_1() {
    let failing = USERS
        .1
        .replace("  - id: list\n    path: /users\n", "  - id: list\n    path: /users\n    expect: {status: 418}\n");
    let (_server, project) = served(&[CONFIG, (USERS.0, &failing)]).await;
    let output = project.vc(&["call", "users#list"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    assert!(said(&output).contains("418"), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn save_as_test_writes_the_status_and_no_schema_and_the_test_passes() {
    let (_server, project) = served(&[CONFIG, USERS]).await;
    let output = project.vc(&["call", "users#list", "--save-as-test", "users.crud#listsUsers"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(
        said(&output).contains("✓ wrote users.crud#listsUsers to services/users/crud.test.yaml:"),
        "{}",
        said(&output)
    );
    let text = project.read(CRUD.0);
    assert!(text.contains("  - id: listsUsers\n") && text.contains("users#list"), "{text}");
    assert!(text.contains("status: 200") && !text.contains("schema"), "D176: status only:\n{text}");
    checks_clean(&project);
    project.vc(&["run", "users.crud"]).assert().code(0);
}

#[tokio::test(flavor = "multi_thread")]
async fn save_as_test_writes_each_expect_field_asked_for() {
    let (_server, project) = served(&[CONFIG, ECHO]).await;
    let args = [
        "call",
        "echo#send",
        "--var",
        "word=hi",
        "--save-as-test",
        "echo.round#echoesWord",
        "--expect-field",
        "body.word",
    ];
    project.vc(&args).assert().code(0);
    let text = project.read("services/echo/round.test.yaml");
    assert!(text.contains("word") && text.contains("hi"), "the field's value is expected:\n{text}");
    project.vc(&["run", "echo.round"]).assert().code(0);
}

#[test]
fn refuses_an_unknown_api_with_the_closest_one() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["call", "users#lst"]).output().unwrap();
    assert!(!output.status.success());
    assert!(said(&output).contains("users#list"), "{}", said(&output));
}
