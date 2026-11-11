//! `expect.timing` checks one phase of an exchange: `firstByte` is the wait for the server's
//! answer, `download` the reading of its body. A failed timing check names its phase in its
//! message (`expect.timing.firstByte`), not only in the excerpt of the file. A phase that did
//! not happen reads 0 ms: `dns` for an IP literal, `tls` over plain HTTP, `dns` and `connect` on
//! a reused connection.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// `timed#slow` answers after 600 ms; `timed#large` sends about 512 KB; `timed#quick` at once.
const TIMED: (&str, &str) = (
    "services/timed/timed.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: slow\n    path: /slow\n    query: {ms: '600'}\n",
        "  - id: large\n    path: /large\n    query: {kb: '512'}\n",
        "  - id: quick\n    path: /slow\n    query: {ms: '0'}\n",
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

/// The mock server is `http://127.0.0.1:<port>`: no name to look up and no TLS.
#[tokio::test(flavor = "multi_thread")]
async fn dns_for_an_ip_literal_and_tls_over_plain_http_read_zero() {
    let (code, text) = checked("quick", "dns: {lt: 1ms}, tls: {eq: 0ms}").await;
    assert_eq!(code, Some(0), "{text}");
    let (code, text) = checked("quick", "dns: {gt: 0ms}").await;
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("expect.timing.dns"), "{text}");
    let (code, text) = checked("quick", "tls: {gt: 0ms}").await;
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("expect.timing.tls"), "{text}");
}

/// The second step to the same host goes over the first step's pooled connection.
#[tokio::test(flavor = "multi_thread")]
async fn connect_on_a_reused_connection_reads_zero() {
    let server = MockServer::start().await.unwrap();
    let test = |second: &str| {
        format!(
            "tests:\n  - id: reusesTheConnection\n    steps:\n      - call: timed#quick\n        expect:\n          timing: {{connect: {{gt: 0ms}}}}\n      - call: timed#quick\n        expect:\n          timing: {{connect: {second}}}\n"
        )
    };
    let project = Project::new(&[CONFIG, TIMED, ("services/timed/reuse.test.yaml", &test("{lt: 5ms}"))]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "a new connection is measured, a reused one is 0: {}", said(&output));
    project.write("services/timed/reuse.test.yaml", &test("{gt: 0ms}"));
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    let text = said(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("expect.timing.connect") && text.contains("got 0"), "{text}");
}
