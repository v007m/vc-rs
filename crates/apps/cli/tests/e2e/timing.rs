//! `expect.timing` checks one phase of an exchange: `firstByte` is the wait for the server's
//! answer, `download` the reading of its body. A failed timing check names its phase in its
//! message (`expect.timing.firstByte`), not only in the excerpt of the file.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// `timed#slow` answers after 600 ms; `timed#large` sends about 512 KB.
const TIMED: (&str, &str) = (
    "services/timed/timed.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: slow\n    path: /slow\n    query: {ms: '600'}\n",
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
    let (code, text) = checked("slow", "firstByte: {gte: 550ms}, total: {gte: 550ms}").await;
    assert_eq!(code, Some(0), "the 600 ms sleep is before the first byte: {text}");
    let (code, text) = checked("slow", "firstByte: {lt: 100ms}").await;
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("expect.timing.firstByte"), "the failure names the phase: {text}");
}

/// The 600 ms wait comes before the first byte, so it counts in `total` but not in `download`.
#[tokio::test(flavor = "multi_thread")]
async fn download_measures_reading_the_body() {
    let (code, text) = checked("large", "download: {lt: 10s}").await;
    assert_eq!(code, Some(0), "{text}");
    let (code, text) = checked("slow", "download: {lt: 400ms}, total: {gte: 550ms}").await;
    assert_eq!(code, Some(0), "the wait is in total, not in download: {text}");
    let (code, text) = checked("slow", "download: {gte: 550ms}").await;
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("expect.timing.download"), "the failure names the phase: {text}");
}
