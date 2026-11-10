use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{call, import, save_call_as_test};
use crate::Code;
use crate::engine_fixture::{TESTS, project};

const SPEC: &str = "openapi: 3.0.3\ninfo: {title: Pets, version: '1'}\nservers: [{url: 'http://127.0.0.1:9'}]\npaths:\n  /pets:\n    get:\n      operationId: listPets\n      responses: {'200': {description: ok}}\n";

#[tokio::test]
async fn a_call_from_the_web_ui_passes_the_agent_policy_and_is_logged() {
    let (_dir, api) = project();
    let sent = call(api.clone(), wire::CallRequest { api: "users#list".into(), ..wire::CallRequest::default() })
        .await
        .unwrap();
    assert_eq!(sent.exit, 1, "{sent:?}");
    let posted = wire::CallRequest { api: "users#create".into(), ..wire::CallRequest::default() };
    assert_eq!(call(api.clone(), posted).await.unwrap_err().code, Code::PermissionDenied);
    let logged = api.engine().agent_actions(0).unwrap();
    assert!(logged.iter().any(|logged| logged.action.agent == "web-ui"), "{logged:?}");
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
