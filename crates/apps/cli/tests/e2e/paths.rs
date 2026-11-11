//! A value in an api's `path` is percent-encoded as one segment (D278): a `/`, a space or a
//! `..` in it reaches the server inside its segment, and a value already piped through
//! `urlEncode` is sent once encoded, with a V4007 warning that the pipe can go.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

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

/// A test that calls `api` with `word` and expects the echoed `path`.
fn arrives(id: &str, api: &str, word: &str, path: &str) -> String {
    format!(
        "  - id: {id}\n    call: paths#{api}\n    vars: {{word: '{word}'}}\n    expect: {{body: {{match: {{path: '{path}'}}}}}}\n"
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_value_arrives_as_one_segment_and_url_encode_is_not_applied_twice() {
    let server = MockServer::start().await.unwrap();
    let tests = [
        arrives("sendsOneSegment", "word", "a/b c", "/echo/a%2Fb%20c/end"),
        arrives("keepsDotsInTheirSegment", "word", "../x", "/echo/..%2Fx/end"),
        arrives("encodesAParentStep", "word", "..", "/echo/%2E%2E/end"),
        arrives("keepsWhatASegmentAllows", "word", "a:b@c=d", "/echo/a:b@c=d/end"),
        arrives("encodesOnce", "encoded", "a/b c", "/echo/a%2Fb%20c"),
    ];
    let test = format!("tests:\n{}", tests.concat());
    let project = Project::new(&[CONFIG, PATHS, ("services/paths/encoding.test.yaml", &test)]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert_eq!(server.stats().await.unwrap().count("GET /echo/{*rest}"), 5);
    let checked = project.vc(&["check"]).output().unwrap();
    let text = said(&checked);
    assert!(text.contains("warning[V4007]: `urlEncode` is not needed in a `path`"), "{text}");
    assert!(text.contains("fix: Remove `| urlEncode` (certain)"), "{text}");
    assert_eq!(text.matches("V4007").count(), 1, "{text}");
}
