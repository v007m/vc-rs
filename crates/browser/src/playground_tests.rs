//! The docs site's example project, checked and run end to end against the demo service in
//! process, as the playground page runs it.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use super::execute;
use crate::testing::Captured;
use crate::{Command, Request, Stream};

/// The example project the playground page starts from.
const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../site/src/examples");

fn examples() -> BTreeMap<String, String> {
    vc_testkit::read_folder(Path::new(EXAMPLES)).unwrap()
}

async fn play(files: BTreeMap<String, String>, command: Command) -> (u8, String, String) {
    let (sink, captured) = Captured::sink();
    let request = Request { files, command, seed: Some(7), ..Request::default() };
    let exit = execute(request, Arc::new(sink)).await;
    let pieces = captured.take();
    (exit, Captured::text(&pieces, Stream::Out), Captured::text(&pieces, Stream::Err))
}

#[tokio::test]
async fn the_example_project_checks_clean() {
    let (exit, out, err) = play(examples(), Command::Check).await;
    assert_eq!(exit, 0, "{out}{err}");
    assert!(out.contains("0 errors"), "{out}");
}

#[tokio::test]
async fn the_example_project_passes_against_the_demo_service() {
    let (exit, out, err) = play(examples(), Command::Run).await;
    assert_eq!(exit, 0, "{out}{err}");
    assert!(out.starts_with("plan  "), "{out}");
    assert!(out.contains("profile local"), "{out}");
    assert!(err.contains("warning: the playground writes no report files"), "{err}");
}

#[tokio::test]
async fn a_broken_value_fails_with_its_excerpt_and_seed() {
    let mut files = examples();
    let search = files.get_mut("services/echo/search.test.yaml").unwrap();
    *search = search.replace("q: ${example.term}", "q: boots");
    let (exit, out, err) = play(files, Command::Run).await;
    assert_eq!(exit, 1, "{out}{err}");
    assert!(out.contains("boots"), "{out}");
    assert!(err.contains("pass `--seed 7`"), "{err}");
}

#[tokio::test]
async fn a_broken_reference_is_a_diagnostic_and_nothing_runs() {
    let mut files = examples();
    let search = files.get_mut("services/echo/search.test.yaml").unwrap();
    *search = search.replace("call: echo#search", "call: echo#serch");
    let (exit, out, err) = play(files, Command::Run).await;
    assert_eq!(exit, 2, "{out}{err}");
    assert!(out.contains("error[V2"), "{out}");
    assert!(err.contains("nothing ran"), "{err}");
}

#[tokio::test]
async fn another_host_is_a_failure_that_says_there_is_no_network() {
    let mut files = examples();
    let config = files.get_mut("vibranium.config.yaml").unwrap();
    *config = config.replacen("echoUrl: http://localhost:7421", "echoUrl: http://example.com", 1);
    let (exit, out, err) = play(files, Command::Run).await;
    assert_eq!(exit, 1, "{out}{err}");
    assert!(out.contains("the playground has no network"), "{out}");
}

#[tokio::test]
async fn a_hook_is_refused_with_help() {
    let mut files = examples();
    let api = "apis:\n  - id: hooked\n    function: hooks/ping.js\n";
    files.insert("services/hooked/hooked.api.yaml".to_owned(), api.to_owned());
    files.insert("hooks/ping.js".to_owned(), "export default () => ({});\n".to_owned());
    let test = "tests:\n  - id: callsHook\n    call: hooked#hooked\n";
    files.insert("services/hooked/calls.test.yaml".to_owned(), test.to_owned());
    let (exit, out, err) = play(files, Command::Run).await;
    assert_ne!(exit, 0, "{out}{err}");
    assert!(!err.contains("internal error"), "{out}{err}");
}

#[tokio::test]
async fn a_request_with_secrets_is_refused() {
    let mut files = examples();
    files.insert(".env.schema".to_owned(), "# @required\nAPI_TOKEN=\n".to_owned());
    let (exit, out, err) = play(files, Command::Run).await;
    assert_eq!(exit, 2, "{out}{err}");
    assert!(err.contains("which the playground cannot load"), "{out}{err}");
}
