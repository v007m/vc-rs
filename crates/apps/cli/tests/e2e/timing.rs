//! `expect.timing` checks one phase of an exchange: `firstByte` is the wait for the server's
//! answer, `download` the reading of its body. A failed timing check names its phase.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// `timed#slow` answers after 300 ms; `timed#large` sends about 512 KB.
const TIMED: (&str, &str) = (
    "services/timed/timed.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: slow\n    path: /slow\n    query: {ms: '300'}\n",
        "  - id: large\n    path: /large\n    query: {kb: '512'}\n",
    ),
);

/// The exit code of a test calling `timed#<api>` that expects `timing: {<check>}`, and what it said.
async fn checked(api: &str, check: &str) -> (Option<i32>, String) {
    let server = MockServer::start().await.unwrap();
    let test = format!("tests:\n  - id: isTimed\n    call: timed#{api}\n    expect:\n      timing: {{{check}}}\n");
    let project = Project::new(&[CONFIG, TIMED, ("services/timed/phases.test.yaml", &test)]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    (output.status.code(), said(&output))
}

#[tokio::test(flavor = "multi_thread")]
async fn first_byte_measures_the_wait_for_the_servers_answer() {
    let (code, text) = checked("slow", "firstByte: {gte: 250ms}, total: {gte: 250ms}").await;
    assert_eq!(code, Some(0), "the 300 ms sleep is before the first byte: {text}");
    let (code, text) = checked("slow", "firstByte: {lt: 100ms}").await;
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("firstByte"), "the failure names the phase: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn download_measures_reading_the_body() {
    let (code, text) = checked("large", "download: {lt: 10s}").await;
    assert_eq!(code, Some(0), "{text}");
    let (code, text) = checked("large", "download: {gte: 1h}").await;
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("download"), "the failure names the phase: {text}");
}
