//! What a `graphql` response's errors hold back (D345, D361), against the mock's `/graphql/reply`:
//! a 200 with errors passes and lists them (V5128) unless `expect.errors` asserts them (D364); the default `save` and the `cleanup` wait for
//! a response without errors (V5129), while an explicit `saveOn` still saves; a 2xx body that is
//! not GraphQL fails (V5124), and any other status is left to the status check.

use std::process::Output;

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// The variables every document declares, so it stays valid (V2108).
const DECLARED: &str = "($reply: JSON, $status: Int)";

/// A user made with partial data and one `FORBIDDEN` error.
const FORBIDDEN: &str = concat!(
    "{data: {createUser: {id: u-7, secretNote: null}}, ",
    "errors: [{message: secretNote is not allowed, path: [createUser, secretNote], extensions: {code: FORBIDDEN}}]}"
);

/// An api `id` the mock answers with `reply` (and `status`, when set), saving the user's id and
/// deleting it afterwards, with `rest` added under it.
fn creating(id: &str, (reply, status): (&str, &str), rest: &str) -> String {
    format!(
        "  - id: {id}\n    body:\n      query: 'mutation M{DECLARED} {{ createUser {{ id secretNote }} }}'\n      \
         variables:\n        reply: {reply}\n        status: {status}\n    save:\n      \
         userId: ${{response.body.data.createUser.id}}\n    cleanup:\n      call: deleteUser\n      vars:\n        \
         userId: ${{response.body.data.createUser.id}}\n{rest}"
    )
}

/// The accounts apis: each way `createUser` can answer, `deleteUser`, and `readUser`, which
/// answers with the `userId` it is given.
fn apis() -> String {
    let fixed = concat!(
        "  - id: deleteUser\n    vars: {userId: none}\n",
        "    body:\n      query: 'mutation D($reply: JSON) { deleteUser }'\n",
        "      variables:\n        reply: {data: {deleteUser: true}}\n",
        "  - id: readUser\n    vars: {userId: none}\n",
        "    body:\n      query: 'query R($reply: JSON) { user { id } }'\n",
        "      variables:\n        reply: {data: {user: {id: '${var.userId}'}}}\n",
    );
    format!(
        "type: graphql\nbaseUrl: ${{var.baseUrl}}\ngraphql:\n  path: /graphql/reply\napis:\n{}{}{}{}{}{fixed}",
        creating("createWithErrors", (FORBIDDEN, "200"), ""),
        creating("createSavingAnyway", (FORBIDDEN, "200"), "    saveOn: 200\n"),
        creating("createCleanly", ("{data: {createUser: {id: u-7, secretNote: hi}}}", "200"), ""),
        creating("createAtHome", ("'<html>home</html>'", "200"), ""),
        creating("createRejected", ("'bad document'", "400"), ""),
    )
}

/// Runs one test of `steps`, returning its output and how many requests the mock answered.
async fn run(steps: &str) -> (Output, u64) {
    let server = MockServer::start().await.unwrap();
    let (apis, tests) = (apis(), format!("tests:\n  - id: makesUser\n    steps:\n{steps}"));
    let files =
        [CONFIG, ("services/accounts/accounts.api.yaml", &*apis), ("services/accounts/users.test.yaml", &*tests)];
    let output = Project::new(&files).run(&server.base_url(), &[]).output().unwrap();
    let sent = server.stats().await.unwrap().count("POST /graphql/reply");
    (output, sent)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_200_with_errors_passes_lists_them_and_skips_the_default_save_and_the_cleanup() {
    let (output, sent) = run("      - call: accounts#createWithErrors\n").await;
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("V5128") && text.contains("FORBIDDEN at createUser.secretNote: secretNote is not allowed"));
    assert!(text.contains("V5129"), "{text}");
    assert!(!text.contains("V5111"), "the save did not run into the nulled field: {text}");
    assert_eq!(sent, 1, "no cleanup was registered");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_save_on_still_saves() {
    let steps = concat!(
        "      - call: accounts#createSavingAnyway\n",
        "      - call: accounts#readUser\n        expect:\n          body:\n            match:\n",
        "              data.user.id: u-7\n",
    );
    let (output, sent) = run(steps).await;
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "the saved userId reached readUser: {text}");
    assert!(text.contains("V5129"), "the cleanup is still held back: {text}");
    assert_eq!(sent, 2, "createSavingAnyway and readUser, no cleanup");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_response_without_errors_saves_and_cleans_up_with_no_warning() {
    let steps = concat!(
        "      - call: accounts#createCleanly\n",
        "      - call: accounts#readUser\n        expect:\n          body:\n            match:\n",
        "              data.user.id: u-7\n",
    );
    let (output, sent) = run(steps).await;
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(!text.contains("V5128") && !text.contains("V5129"), "{text}");
    assert_eq!(sent, 3, "createCleanly, readUser and the cleanup's deleteUser");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_2xx_body_that_is_not_graphql_is_v5124_and_a_400_is_left_to_the_status_check() {
    let (output, sent) = run("      - call: accounts#createAtHome\n").await;
    let text = said(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("V5124") && text.contains("not a GraphQL response"), "{text}");
    assert_eq!(sent, 1, "no cleanup after a body that is not GraphQL");
    let (output, _) = run("      - call: accounts#createRejected\n").await;
    let text = said(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("V5101") && !text.contains("V5124"), "the status check fails: {text}");
    let expected = "      - call: accounts#createRejected\n        expect:\n          status: 400\n";
    let (output, _) = run(expected).await;
    assert_eq!(output.status.code(), Some(0), "a 400 it expects passes: {}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_a_passing_expect_errors_asserts_are_not_listed() {
    let steps = "      - call: accounts#createWithErrors\n        expect:\n          errors: any\n";
    let (output, _) = run(steps).await;
    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(!text.contains("V5128") && text.contains("V5129"), "D364: asserted, but the save is still held: {text}");
}
