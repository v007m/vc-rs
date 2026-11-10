//! `vc call` sends one api ad hoc with the active profile and auth (D178): it prints the
//! status line and the body, runs the api's cleanup unless `--keep`, records the call in the
//! history as `adhoc`, skips `beforeRun` and `beforeAll`, and exits 1 when the api's expect fails.

use serde_json::Value;
use vc_mock_server::{MockHandle, MockServer};

use crate::{CONFIG, Project, USERS, said};

/// `echo#send`, posting `{word: ${var.word}}` to the mock's `/echo`, which answers with the
/// request it got.
const ECHO: (&str, &str) = (
    "services/echo/echo.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: send\n    method: POST\n    path: /echo\n    vars: {word: none}\n    body: {word: '${var.word}'}\n",
);

/// A project whose config points at a fresh mock server, with `extra` appended to the config.
async fn served(files: &[(&str, &str)], extra: &str) -> (MockHandle, Project) {
    let server = MockServer::start().await.unwrap();
    let config = format!("{}{extra}", CONFIG.1.replace("http://x", &server.base_url()));
    let project = Project::new(files);
    project.write(CONFIG.0, &config);
    (server, project)
}

#[tokio::test(flavor = "multi_thread")]
async fn calls_one_api_and_prints_the_response() {
    let (_server, project) = served(&[USERS], "").await;
    let output = project.vc(&["call", "users#list"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("200 OK · ") && stdout.contains(" ms · api expect passed\n"), "{stdout}");
}

#[tokio::test(flavor = "multi_thread")]
async fn vars_on_the_command_line_reach_the_request() {
    let (_server, project) = served(&[ECHO], "").await;
    let output = project.vc(&["call", "echo#send", "--var", "word=hello"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(String::from_utf8_lossy(&output.stdout).contains("\"word\": \"hello\""), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_the_cleanup_afterwards_unless_kept() {
    let (server, project) = served(&[USERS], "").await;
    project.vc(&["call", "users#create"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("POST /users"), stats.count("DELETE /users/{id}")), (1, 1), "{stats:?}");
    project.vc(&["call", "users#create", "--keep"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("POST /users"), stats.count("DELETE /users/{id}")), (2, 1), "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn skips_before_run_and_before_all() {
    let (server, project) = served(&[USERS], "beforeRun:\n  - call: users#create\n").await;
    project.vc(&["call", "users#list"]).assert().code(0);
    let stats = server.stats().await.unwrap();
    assert_eq!((stats.count("GET /users"), stats.count("POST /users")), (1, 0), "{stats:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn records_the_call_in_history_as_adhoc() {
    let (_server, project) = served(&[USERS], "").await;
    project.vc(&["call", "users#list"]).assert().code(0);
    let output = project.vc(&["history", "--json"]).output().unwrap();
    let runs: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(runs.as_array().map(Vec::len), Some(1), "{runs:#}");
    assert_eq!((&runs[0]["kind"], &runs[0]["selector"]), (&"adhoc".into(), &"users#list".into()), "{runs:#}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_api_expect_exits_1() {
    let failing = USERS
        .1
        .replace("    path: /users\n  - id: create", "    path: /users\n    expect: {status: 418}\n  - id: create");
    let (_server, project) = served(&[(USERS.0, &failing)], "").await;
    let output = project.vc(&["call", "users#list"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("api expect failed") && stdout.contains("418"), "{}", said(&output));
}

#[test]
fn refuses_an_unknown_api_with_the_closest_one() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["call", "users#lst"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("users#list"), "{}", said(&output));
}

#[test]
fn refuses_a_var_without_a_name() {
    let project = Project::new(&[CONFIG, USERS]);
    let output = project.vc(&["call", "users#list", "--var", "=7"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    assert!(said(&output).contains("--var userId=7"), "{}", said(&output));
}
