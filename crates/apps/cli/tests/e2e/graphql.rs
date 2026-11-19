//! `graphql` apis (D343, D344) sent to the mock's `/graphql/echo`, which answers with the method,
//! path, headers and body it received, as a GraphQL response: the file's `graphql.path`, an api's own `path` over it, the JSON
//! body with its variables rendered and its query sent as written, the implied headers, and a
//! `content-type` a step sets sent as written.

use std::process::Output;

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

/// `lab#getUser` on the file's path, `lab#ownPath` on its own, both on a `type: graphql` file.
const API: (&str, &str) = (
    "services/lab/lab.api.yaml",
    concat!(
        "type: graphql\nbaseUrl: ${var.baseUrl}\ngraphql:\n  path: /graphql/echo\napis:\n",
        "  - id: getUser\n    vars: {userId: '1'}\n    body:\n",
        "      query: 'query GetUser($id: ID!) { user(id: $id) { id note(text: \"${x}\") } }'\n",
        "      variables: {id: '${var.userId}', limit: 2}\n      operationName: GetUser\n",
        "  - id: ownPath\n    path: /graphql/echo/own\n    body:\n      query: '{ me { id } }'\n",
    ),
);

/// Runs `tests` (a test file's text) calling `lab`; gives the output and the requests `/graphql/echo` saw.
async fn run(tests: &str) -> (Output, u64) {
    let server = MockServer::start().await.unwrap();
    let files = [CONFIG, API, ("services/lab/graphql.test.yaml", tests)];
    let output = Project::new(&files).run(&server.base_url(), &[]).output().unwrap();
    let stats = server.stats().await.unwrap();
    (output, stats.count("POST /graphql/echo") + stats.count("POST /graphql/echo/{*rest}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_graphql_api_posts_its_query_and_rendered_variables_as_json_to_the_files_path() {
    let tests = concat!(
        "tests:\n  - id: sendsTheOperation\n    call: lab#getUser\n    vars: {userId: '42'}\n    expect:\n",
        "      body:\n        match:\n          method: POST\n          path: /graphql/echo\n",
        "          body.query: 'query GetUser($id: ID!) { user(id: $id) { id note(text: \"$${x}\") } }'\n",
        "          body.variables: {id: '42', limit: 2}\n          body.operationName: GetUser\n",
        "          \"headers['content-type']\": application/json\n",
        "          \"headers['accept']\": 'application/graphql-response+json, application/json;q=0.9'\n",
    );
    let (output, sent) = run(tests).await;
    assert_eq!((output.status.code(), sent), (Some(0), 1), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_apis_own_path_wins_and_unset_variables_are_sent_empty_without_an_operation_name() {
    let tests = concat!(
        "tests:\n  - id: sendsToItsOwnPath\n    call: lab#ownPath\n    expect:\n",
        "      body:\n        match:\n          path: /graphql/echo/own\n          body.variables: {}\n",
        "      cel:\n        sends no operation name: '!has(response.body.body.operationName)'\n",
    );
    let (output, sent) = run(tests).await;
    assert_eq!((output.status.code(), sent), (Some(0), 1), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_content_type_and_accept_a_step_sets_are_sent_as_written() {
    let tests = concat!(
        "tests:\n  - id: sendsTheWrittenHeaders\n    call: lab#ownPath\n",
        "    headers: {content-type: text/plain, accept: application/json}\n    expect:\n",
        "      body:\n        match:\n          \"headers['content-type']\": text/plain\n",
        "          \"headers['accept']\": application/json\n          body.query: '{ me { id } }'\n",
    );
    let (output, sent) = run(tests).await;
    assert_eq!((output.status.code(), sent), (Some(0), 1), "{}", said(&output));
}
