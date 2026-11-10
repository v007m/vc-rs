//! Request bodies the syntax reference promises beyond JSON and text: a `$file` body sent as the
//! file's bytes, a map sent as a URL-encoded form, and a map with `$file` values sent as
//! multipart parts. The mock's `/echo` answers with the headers and body it received.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

const USERS_CSV: (&str, &str) = ("fixtures/users.csv", "id,name\n1,Ada\n2,Grace\n");
const NOTE: (&str, &str) = ("fixtures/note.txt", "a note for the upload\n");

/// Runs a test calling `lab#send`, whose `api` posts to `/echo` and checks what was echoed.
async fn sends(api: &str) -> std::process::Output {
    let server = MockServer::start().await.unwrap();
    let apis = format!("baseUrl: ${{var.baseUrl}}\napis:\n  - id: send\n    method: POST\n    path: /echo\n{api}");
    let test = "tests:\n  - id: sendsTheBody\n    call: lab#send\n";
    let files = [CONFIG, USERS_CSV, NOTE, ("services/lab/lab.api.yaml", &apis), ("services/lab/body.test.yaml", test)];
    let output = Project::new(&files).run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 1, "{}", said(&output));
    output
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_body_is_sent_as_the_files_bytes_with_its_extensions_type() {
    let api = "    body: {$file: ../../fixtures/users.csv}\n    expect:\n      body:\n        match:\n\
               \x20         body: \"id,name\\n1,Ada\\n2,Grace\\n\"\n          \"headers['content-type']\": {startsWith: text/csv}\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

#[ignore = "issue: form bodies are sent as JSON until D273 lands"]
#[tokio::test(flavor = "multi_thread")]
async fn a_map_with_a_form_content_type_is_sent_url_encoded() {
    let api = "    headers: {content-type: application/x-www-form-urlencoded}\n    body: {a: one two, b: \"2\"}\n\
               \x20   expect:\n      body:\n        match:\n          body: a=one+two&b=2\n\
               \x20         \"headers['content-type']\": application/x-www-form-urlencoded\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

#[ignore = "issue: multipart bodies are sent as JSON until D274 lands"]
#[tokio::test(flavor = "multi_thread")]
async fn a_map_with_a_multipart_content_type_sends_file_and_text_parts() {
    let api = "    headers: {content-type: multipart/form-data}\n\
               \x20   body: {file: {$file: ../../fixtures/note.txt}, caption: x}\n    expect:\n      body:\n        match:\n\
               \x20         \"headers['content-type']\": {startsWith: 'multipart/form-data; boundary='}\n\
               \x20         body: {allOf: [{contains: 'name=\"file\"; filename=\"note.txt\"'}, \
               {contains: a note for the upload}, {contains: 'name=\"caption\"'}]}\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}
