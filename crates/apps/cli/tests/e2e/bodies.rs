//! Request bodies the syntax reference promises beyond JSON and text: a `$file` body sent as the
//! file's bytes, a map sent as a URL-encoded form, and a map with `$file` values sent as
//! multipart parts, also under `$extend` and from a cleanup that `vc cache clear` runs. A JSON
//! body's `$file` map is sent as written, and a part's `$file` path must be written in the body.
//! The mock's `/echo` answers with the headers and body it received.

use std::process::Output;

use vc_mock_server::MockServer;

use crate::{Project, said};

/// A config whose vars also hold a map to `$extend` and a `$file` map to pass by reference.
const VARS: (&str, &str) = (
    "vibranium.config.yaml",
    "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n\
     vars: {baseUrl: 'http://x', word: note, base: {kept: yes}, payload: {$file: fixtures/note.txt}}\n",
);

const USERS_CSV: (&str, &str) = ("fixtures/users.csv", "id,name\n1,Ada\n2,Grace\n");
const NOTE: (&str, &str) = ("fixtures/note.txt", "a note for the upload\n");

/// Runs a test calling `lab#send`, whose `api` posts to `/echo` and checks what was echoed.
async fn sends(api: &str) -> Output {
    let (output, sent) = sent(api).await;
    assert_eq!(sent, 1, "{}", said(&output));
    output
}

/// Runs a test calling `lab#send`, whose `api` posts to `/echo`, under the [`VARS`] config;
/// gives the run's output and how many requests reached `/echo`.
async fn sent(api: &str) -> (Output, u64) {
    let server = MockServer::start().await.unwrap();
    let apis = format!("baseUrl: ${{var.baseUrl}}\napis:\n  - id: send\n    method: POST\n    path: /echo\n{api}");
    let test = "tests:\n  - id: sendsTheBody\n    call: lab#send\n";
    let files = [VARS, USERS_CSV, NOTE, ("services/lab/lab.api.yaml", &apis), ("services/lab/body.test.yaml", test)];
    let output = Project::new(&files).run(&server.base_url(), &[]).output().unwrap();
    (output, server.stats().await.unwrap().count("POST /echo"))
}

/// The output of a run whose request is not sent because its body fails with V5111.
async fn refused(api: &str) -> String {
    let (output, sent) = sent(api).await;
    let said = said(&output);
    assert_eq!((output.status.code(), sent), (Some(1), 0), "{said}");
    assert!(said.contains("V5111"), "{said}");
    said
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_body_is_sent_as_the_files_bytes_with_its_extensions_type() {
    let api = "    body: {$file: ../../fixtures/users.csv}\n    expect:\n      body:\n        match:\n\
               \x20         body: \"id,name\\n1,Ada\\n2,Grace\\n\"\n          \"headers['content-type']\": {startsWith: text/csv}\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_map_with_a_form_content_type_is_sent_url_encoded() {
    let api = "    headers: {content-type: application/x-www-form-urlencoded}\n    body: {a: one two, b: \"2\"}\n\
               \x20   expect:\n      body:\n        match:\n          body: a=one+two&b=2\n\
               \x20         \"headers['content-type']\": application/x-www-form-urlencoded\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

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

#[tokio::test(flavor = "multi_thread")]
async fn an_extended_multipart_body_sends_its_file_part() {
    let api = "    headers: {content-type: multipart/form-data}\n\
               \x20   body: {$extend: '${var.base}', file: {$file: ../../fixtures/note.txt}}\n    expect:\n      body:\n\
               \x20       match:\n          body: {allOf: [{contains: 'filename=\"note.txt\"'}, \
               {contains: a note for the upload}, {contains: 'name=\"kept\"'}]}\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_json_body_sends_its_file_map_as_written() {
    let api = "    body: {doc: {$file: ../../fixtures/note.txt}}\n    expect:\n      body:\n        match:\n\
               \x20         \"body.doc['$file']\": ../../fixtures/note.txt\n\
               \x20         \"headers['content-type']\": application/json\n";
    let output = sends(api).await;
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_multipart_file_part_with_a_templated_path_fails_with_v5111() {
    let api = "    headers: {content-type: multipart/form-data}\n    body: {file: {$file: '../../fixtures/${var.word}.txt'}}\n";
    let said = refused(api).await;
    assert!(said.contains("cannot hold `${…}` references in its path"), "{said}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_multipart_file_part_from_a_reference_fails_with_v5111() {
    let api = "    headers: {content-type: multipart/form-data}\n    body: {file: '${var.payload}'}\n";
    let said = refused(api).await;
    assert!(said.contains("arrives in a `${…}` value"), "{said}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cleanup_that_cache_clear_runs_sends_its_file_body() {
    let server = MockServer::start().await.unwrap();
    let config = format!(
        "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {{baseUrl: '{}'}}\n\
         beforeRun:\n  - id: made\n    call: lab#make\n    cache: {{for: 1h}}\n",
        server.base_url()
    );
    let apis = "baseUrl: ${var.baseUrl}\napis:\n  - id: make\n    method: POST\n    path: /echo\n\
                \x20   cleanup: {call: drop}\n  - id: drop\n    method: PUT\n    path: /echo\n\
                \x20   body: {$file: ../../fixtures/note.txt}\n\
                \x20   expect: {body: {match: {body: {contains: a note for the upload}}}}\n\
                \x20 - id: peek\n    path: /echo\n";
    let test = "tests:\n  - id: peeks\n    call: lab#peek\n";
    let config = ("vibranium.config.yaml", config.as_str());
    let files = [config, NOTE, ("services/lab/lab.api.yaml", apis), ("services/lab/a.test.yaml", test)];
    let project = Project::new(&files);
    let mut run = project.vc(&["run"]);
    let run = run.env("VIB_CACHE_KEY", KEY).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    assert_eq!(server.stats().await.unwrap().count("PUT /echo"), 0, "the cleanup waits for the clear");
    let mut clear = project.vc(&["cache", "clear"]);
    let clear = clear.env("VIB_CACHE_KEY", KEY).output().unwrap();
    assert_eq!(clear.status.code(), Some(0), "{}", said(&clear));
    assert!(!said(&clear).contains("failed"), "the cleanup's `$file` body was read: {}", said(&clear));
    assert_eq!(server.stats().await.unwrap().count("PUT /echo"), 1, "{}", said(&clear));
}

const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
