//! `expect.errors` on `graphql` apis (D345), against the mock's `/graphql/reply`, which answers
//! with the `reply` its variables carry: no implicit errors check, the `none`, `any`, list and
//! matcher forms, and a test's `errors` replacing the api's.

use std::process::Output;

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// The variables every api declares, so its document stays valid (V2108).
const DECLARED: &str = "($id: ID!, $reply: JSON)";

/// `accounts#getSecretNote` answers the §1 example's partial data with one `FORBIDDEN` error;
/// `accounts#getTwoNotes` adds a `TIMEOUT` one; `accounts#getStrictly` expects no errors.
fn apis() -> String {
    let forbidden = "{message: secretNote is not allowed, path: [user, secretNote], extensions: {code: FORBIDDEN}}";
    let timeout = "{message: notes timed out, path: [user, notes, 0], extensions: {code: TIMEOUT, ms: 500}}";
    let api = |id: &str, errors: &str, rest: &str| {
        format!(
            "  - id: {id}\n    body:\n      query: 'query Q{DECLARED} {{ user(id: $id) {{ name secretNote }} }}'\n      \
             variables:\n        id: '2'\n        reply:\n          data: {{user: {{name: Grace Hopper, secretNote: null}}}}\n          \
             errors: [{errors}]\n{rest}"
        )
    };
    format!(
        "type: graphql\nbaseUrl: ${{var.baseUrl}}\ngraphql:\n  path: /graphql/reply\napis:\n{}{}{}",
        api("getSecretNote", forbidden, ""),
        api("getTwoNotes", &format!("{forbidden}, {timeout}"), ""),
        api("getStrictly", forbidden, "    expect:\n      errors: none\n"),
    )
}

/// Runs one test calling `api` with `expect` (indented under the test's `expect:`).
async fn run(api: &str, expect: &str) -> Output {
    let server = MockServer::start().await.unwrap();
    let (apis, tests) =
        (apis(), format!("tests:\n  - id: checksErrors\n    call: accounts#{api}\n    expect:\n{expect}"));
    let files =
        [CONFIG, ("services/accounts/accounts.api.yaml", &*apis), ("services/accounts/privacy.test.yaml", &*tests)];
    Project::new(&files).run(&server.base_url(), &[]).output().unwrap()
}

/// The exit code, and whether the output names V5123.
fn verdict(output: &Output) -> (Option<i32>, bool) {
    (output.status.code(), said(output).contains("V5123"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_200_with_errors_passes_unless_expect_errors_says_otherwise() {
    let output = run("getSecretNote", "      status: 200\n").await;
    assert_eq!(verdict(&output), (Some(0), false), "{}", said(&output));
    let output = run("getSecretNote", "      errors: none\n").await;
    assert_eq!(verdict(&output), (Some(1), true), "{}", said(&output));
    let output = run("getSecretNote", "      errors: any\n").await;
    assert_eq!(verdict(&output), (Some(0), false), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_list_form_needs_each_entry_matched_and_each_error_listed() {
    let entries = concat!(
        "      errors:\n        - code: FORBIDDEN\n          path: user.secretNote\n",
        "          message: { contains: not allowed }\n",
        "      body:\n        match:\n          data.user.name: Grace Hopper\n          data.user.secretNote: null\n",
    );
    let output = run("getSecretNote", entries).await;
    assert_eq!(verdict(&output), (Some(0), false), "hidesSecretNote passes: {}", said(&output));
    let output = run("getTwoNotes", entries).await;
    assert_eq!(verdict(&output), (Some(1), true), "the TIMEOUT error is not listed: {}", said(&output));
    let both = "      errors:\n        - {code: FORBIDDEN}\n        - {path: 'user.notes[0]', extensions: {code: TIMEOUT, ms: 500}}\n";
    let output = run("getTwoNotes", both).await;
    assert_eq!(verdict(&output), (Some(0), false), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_matcher_map_holds_on_the_normalised_list() {
    let timeout = "      errors: { some: { code: TIMEOUT } }\n";
    let output = run("getTwoNotes", timeout).await;
    assert_eq!(verdict(&output), (Some(0), false), "{}", said(&output));
    let output = run("getSecretNote", timeout).await;
    assert_eq!(verdict(&output), (Some(1), true), "{}", said(&output));
    let output = run("getTwoNotes", "      errors: { length: 2 }\n").await;
    assert_eq!(verdict(&output), (Some(0), false), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tests_errors_replaces_the_apis() {
    let output = run("getStrictly", "      status: 200\n").await;
    assert_eq!(verdict(&output), (Some(1), true), "the api's `errors: none` applies: {}", said(&output));
    let output = run("getStrictly", "      errors: [{code: FORBIDDEN}]\n").await;
    assert_eq!(verdict(&output), (Some(0), false), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_entry_takes_only_the_error_keys() {
    let output = run("getSecretNote", "      errors: [{cod: FORBIDDEN}]\n").await;
    let text = said(&output);
    assert!(text.contains("V1004") && text.contains("`code`"), "a close name is offered: {text}");
    let output = run("getSecretNote", "      errors: { some: { mesage: no } }\n").await;
    assert!(said(&output).contains("V1004"), "{}", said(&output));
    let output = run("getSecretNote", "      errors: [FORBIDDEN]\n").await;
    assert!(said(&output).contains("V2306"), "{}", said(&output));
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
}
