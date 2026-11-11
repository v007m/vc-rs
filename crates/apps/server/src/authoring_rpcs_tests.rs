use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{call, import, save_call_as_test};
use crate::Code;
use crate::engine_fixture::{API, CONFIG, TESTS, USERS, large_body_server, project, serving};

/// A request body this long is past the test's `history.maxBodySize` of 1kb.
const PAD: usize = 1500;

/// The test's `history.maxBodySize`.
const KB: usize = 1024;

const SPEC: &str = "openapi: 3.0.3\ninfo: {title: Pets, version: '1'}\nservers: [{url: 'http://127.0.0.1:9'}]\npaths:\n  /pets:\n    get:\n      operationId: listPets\n      responses: {'200': {description: ok}}\n";

#[tokio::test]
async fn a_call_from_the_web_ui_skips_the_agent_policy_and_the_agent_log() {
    let (_dir, api) = project();
    for id in ["users#list", "users#create"] {
        let sent = call(api.clone(), wire::CallRequest { api: id.into(), ..wire::CallRequest::default() }).await;
        assert_eq!(sent.unwrap().exit, 1, "{id} is sent, though the policy lets agents send only GET");
    }
    let logged = api.engine().agent_actions(0).unwrap();
    assert!(logged.is_empty(), "{logged:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_sends_its_exchange_with_both_bodies_cut_at_max_body_size_and_its_checks() {
    let (dir, api) = project();
    let config = CONFIG.replace("http://127.0.0.1:9", &large_body_server(200));
    let config = format!("{config}history: {{bodies: none, maxBodySize: 1kb}}\n");
    fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    let padded =
        USERS.replacen("    path: /users\n", &format!("    path: /users\n    body: {{pad: {}}}\n", "p".repeat(PAD)), 1);
    fs::write(dir.path().join(API), padded).unwrap();
    let request = wire::CallRequest { api: "users#list".into(), ..wire::CallRequest::default() };
    let sent = call(api.clone(), request.clone()).await.unwrap();
    let exchange = sent.exchange.unwrap();
    assert_eq!((exchange.status, exchange.method.as_str()), (Some(200), "GET"));
    let lengths = (exchange.request_body.len(), exchange.response_body.len(), exchange.body_truncated);
    assert_eq!(lengths, (KB, KB, true), "both bodies cut at history.maxBodySize, though bodies: none");
    fs::write(dir.path().join(API), USERS).unwrap();
    let alone = call(api, request).await.unwrap().exchange.unwrap();
    let lengths = (alone.request_body.len(), alone.response_body.len(), alone.body_truncated);
    assert_eq!(lengths, (0, KB, true), "one body cut is enough to flag it");
    assert_eq!((exchange.action.as_str(), exchange.api.as_deref()), ("call users#list", Some("users#list")));
    assert!(sent.checks.iter().all(|check| check.passed) && sent.saved.is_empty(), "{:?}", sent.checks);
}

/// The session token the api answers with, inside a field `redact.fields` masks whole: test
/// data, in no real provider's key format.
const SESSION: &str = "sess-fixture-3b7a90";

#[tokio::test(flavor = "multi_thread")]
async fn a_value_saved_from_inside_a_field_masked_whole_is_sent_masked() {
    let (dir, api) = project();
    let body = format!(r#"{{"session": {{"token": "{SESSION}"}}}}"#);
    let config = CONFIG.replace("http://127.0.0.1:9", &serving(200, body.into_bytes()));
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{config}redact:\n  fields: [session]\n")).unwrap();
    let saving = "    path: /users\n    save:\n      sessionToken: ${response.body.session.token}\n";
    fs::write(dir.path().join(API), USERS.replacen("    path: /users\n", saving, 1)).unwrap();
    let sent = call(api, wire::CallRequest { api: "users#list".into(), ..wire::CallRequest::default() }).await;
    let sent = sent.unwrap();
    let saved: Vec<(&str, &str)> = sent.saved.iter().map(|saved| (saved.name.as_str(), saved.value.as_str())).collect();
    assert_eq!(saved, [("sessionToken", "\"••••\"")]);
    assert!(!format!("{sent:?}").contains(SESSION), "the token leaks: {sent:?}");
}

#[tokio::test]
async fn a_call_that_got_no_response_sends_no_exchange() {
    let (_dir, api) = project();
    let sent = call(api, wire::CallRequest { api: "users#list".into(), ..wire::CallRequest::default() }).await;
    assert!(sent.unwrap().exchange.is_none());
}

#[tokio::test]
async fn a_saved_call_is_written_as_a_test_and_bad_json_is_refused() {
    let (dir, api) = project();
    let bad = wire::SaveCallAsTestRequest { response_json: Some("{".into()), ..wire::SaveCallAsTestRequest::default() };
    assert_eq!(save_call_as_test(api.clone(), bad).await.unwrap_err().code, Code::InvalidArgument);
    let request = wire::SaveCallAsTestRequest {
        id: "users.crud#greetsUser".into(),
        api: "users#list".into(),
        response_json: Some(r#"{"status":200,"headers":{},"body":{"word":"hi"}}"#.into()),
        fields: vec!["body.word".into()],
        ..wire::SaveCallAsTestRequest::default()
    };
    let saved = save_call_as_test(api, request).await.unwrap();
    assert_eq!((saved.id.as_str(), saved.path.as_str()), ("users.crud#greetsUser", TESTS));
    assert!(fs::read_to_string(dir.path().join(TESTS)).unwrap().contains("greetsUser"));
}

#[tokio::test]
async fn an_import_previews_the_pasted_spec_and_writes_only_when_asked() {
    let (dir, api) = project();
    let request = wire::ImportRequest {
        kind: "openapi".into(),
        source: SPEC.into(),
        file_id: Some("pets".into()),
        ..wire::ImportRequest::default()
    };
    let preview = import(api.clone(), request.clone()).await.unwrap();
    let api_file = preview.files.iter().find(|file| file.path.ends_with("pets.api.yaml")).unwrap();
    assert!(api_file.before.is_none() && api_file.after.contains("listPets"), "{preview:?}");
    assert!(!dir.path().join(&api_file.path).exists());
    let written = import(api, wire::ImportRequest { write: true, ..request }).await.unwrap();
    assert!(written.files.iter().all(|file| dir.path().join(&file.path).exists()));
}

#[tokio::test]
async fn a_pasted_spec_cannot_pull_in_the_env_file() {
    let (_dir, api) = project();
    let spec = "openapi: 3.0.3\ninfo: {title: Pets, version: '1'}\npaths:\n  /pets:\n    $ref: '.env'\n";
    let request = wire::ImportRequest { kind: "openapi".into(), source: spec.into(), ..wire::ImportRequest::default() };
    let refused = import(api, request).await.unwrap_err();
    assert!(!format!("{refused:?}").contains(crate::engine_fixture::SECRET), "{refused:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pasted_spec_cannot_make_the_server_fetch_a_url() {
    let (_dir, api) = project();
    let spec = "openapi: 3.0.3\ninfo: {title: Pets, version: '1'}\npaths:\n  /pets:\n    $ref: 'http://169.254.169.254/latest/meta-data'\n";
    let request = wire::ImportRequest { kind: "openapi".into(), source: spec.into(), ..wire::ImportRequest::default() };
    let refused = import(api, request).await.unwrap_err();
    assert!(refused.message.contains("may not fetch URLs"), "{refused:?}");
}
