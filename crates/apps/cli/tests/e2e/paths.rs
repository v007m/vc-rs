//! A value in an api's `path` is percent-encoded as one segment (D278): a `/`, a space or a
//! `..` in it reaches the server inside its segment, and a value already piped through
//! `urlEncode` is sent once encoded, with a V4007 warning that the pipe can go. A value that
//! starts a path is sent as written only when it is an absolute `http(s)://` URL.

use vc_mock_server::MockServer;

use crate::{Project, said};

/// The default profile, with `echoUrl`, an absolute URL that `direct#kept` starts its path with.
const CONFIG: (&str, &str) = (
    "vibranium.config.yaml",
    concat!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n",
        "vars: {baseUrl: 'http://x', echoUrl: '${var.baseUrl}/echo', word: x}\n",
    ),
);

/// `paths#word` sends `${var.word}` as is; `paths#encoded` pipes it through `urlEncode` first.
/// The mock's `/echo/…` answers with the path as it arrived.
const PATHS: (&str, &str) = (
    "services/paths/paths.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\napis:\n",
        "  - id: word\n    path: /echo/${var.word}/end\n",
        "  - id: encoded\n    path: /echo/${var.word | urlEncode}\n",
    ),
);

/// `nested#orders` starts its path with `${var.word}`, under a `baseUrl` that ends in `/echo/`.
const NESTED: (&str, &str) = (
    "services/nested/nested.api.yaml",
    "baseUrl: ${var.baseUrl}/echo/\napis:\n  - id: orders\n    path: ${var.word}/orders\n",
);

/// `direct#kept` has no `baseUrl`: its path starts with the absolute `${var.echoUrl}`.
const DIRECT: (&str, &str) =
    ("services/direct/direct.api.yaml", "apis:\n  - id: kept\n    path: ${var.echoUrl}/kept\n");

/// A test that calls `api` with `word` and expects the echoed `path`.
fn arrives(id: &str, api: &str, word: &str, path: &str) -> String {
    format!(
        "  - id: {id}\n    call: {api}\n    vars: {{word: '{word}'}}\n    expect: {{body: {{match: {{path: '{path}'}}}}}}\n"
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_value_arrives_as_one_segment_and_url_encode_is_not_applied_twice() {
    let server = MockServer::start().await.unwrap();
    let tests = [
        arrives("sendsOneSegment", "paths#word", "a/b c", "/echo/a%2Fb%20c/end"),
        arrives("keepsDotsInTheirSegment", "paths#word", "../x", "/echo/..%2Fx/end"),
        arrives("encodesAParentStep", "paths#word", "..", "/echo/%2E%2E/end"),
        arrives("keepsWhatASegmentAllows", "paths#word", "a:b@c=d", "/echo/a:b@c=d/end"),
        arrives("encodesOnce", "paths#encoded", "a/b c", "/echo/a%2Fb%20c"),
        arrives("encodesALeadingValue", "nested#orders", "../admin", "/echo/..%2Fadmin/orders"),
        arrives("keepsALeadingUrl", "direct#kept", "x", "/echo/kept"),
    ];
    let test = format!("tests:\n{}", tests.concat());
    let project = Project::new(&[CONFIG, PATHS, NESTED, DIRECT, ("services/paths/encoding.test.yaml", &test)]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(server.stats().await.unwrap().count("GET /echo/{*rest}"), 7);
    let checked = project.vc(&["check"]).output().unwrap();
    let warned = said(&checked);
    assert!(warned.contains("warning[V4007]: `urlEncode` is not needed in a `path`"), "{warned}");
    assert!(warned.contains("fix: Remove `| urlEncode` (certain)"), "{warned}");
    assert_eq!(warned.matches("V4007").count(), 1, "{warned}");
}
