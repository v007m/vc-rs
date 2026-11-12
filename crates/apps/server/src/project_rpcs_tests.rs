use prost::Message as _;
use vc_contracts::engine::v1 as wire;

use super::{check, check_secrets, explain_config, get_project, list_definitions, read_file, test_auth, write_file};
use crate::Code;
use crate::engine_fixture::{API, CONFIG, CRUD, SECRET, TESTS, USERS, project};

#[tokio::test]
async fn get_project_names_the_project_its_profiles_and_the_default_one() {
    let (_dir, api) = project();
    let found = get_project(api, wire::GetProjectRequest::default()).await.unwrap();
    assert_eq!(found.name, "shop");
    assert_eq!(found.profiles, ["dev", "bare"]);
    assert_eq!(found.default_profile.as_deref(), Some("dev"));
    assert_ne!(found.vc_version, "");
}

#[tokio::test]
async fn list_definitions_gives_each_test_with_its_span_and_the_apis_it_calls() {
    let (_dir, api) = project();
    let request = wire::ListDefinitionsRequest {
        kind: wire::DefinitionKind::Tests.into(),
        selection: Some(wire::Selection { tags: vec!["smoke".into()], ..wire::Selection::default() }),
        ..wire::ListDefinitionsRequest::default()
    };
    let found = list_definitions(api, request).await.unwrap();
    let [test] = found.definitions.as_slice() else { panic!("{found:?}") };
    assert_eq!((test.id.as_str(), test.path.as_str()), ("users.crud#listsUsers", TESTS));
    assert_eq!(test.span.as_ref().map(|span| (span.file.as_str(), span.start_line)), Some((TESTS, 2)));
    let call = test.calls.first().unwrap();
    assert_eq!((call.api.as_str(), call.method.as_str(), call.path.as_str()), ("users#list", "GET", "/users"));
}

#[tokio::test]
async fn list_definitions_gives_a_skip_with_its_reason_and_until_and_one_past_its_until_as_ended() {
    let (dir, api) = project();
    let skips = "tests:\n  - id: listsUsers\n    call: users#list\n    skip: {reason: flaky upstream, until: 2099-12-01}\n  - id: listsAgain\n    call: users#list\n    skip: {reason: old outage, until: 2020-01-01}\n";
    std::fs::write(dir.path().join(TESTS), skips).unwrap();
    let request = wire::ListDefinitionsRequest {
        kind: wire::DefinitionKind::Tests.into(),
        ..wire::ListDefinitionsRequest::default()
    };
    let found = list_definitions(api, request).await.unwrap();
    let skips: Vec<String> = found
        .definitions
        .iter()
        .map(|test| format!("{} {:?} {:?} {}", test.skipped, test.skip_reason, test.skip_until, test.skip_expired))
        .collect();
    assert_eq!(
        skips,
        [
            r#"true Some("flaky upstream") Some("2099-12-01") false"#,
            r#"false Some("old outage") Some("2020-01-01") true"#
        ]
    );
}

#[tokio::test]
async fn check_compiles_unsaved_text_in_place_of_the_file() {
    let (_dir, api) = project();
    let broken = "tests:\n  - id: listsUsers\n    call: users#lst\n";
    let request =
        wire::CheckRequest { overlay: [(TESTS.to_owned(), broken.to_owned())].into(), ..wire::CheckRequest::default() };
    let found = check(api, request).await.unwrap();
    let error = found.diagnostics.iter().find(|found| found.severity() == wire::Severity::Error).unwrap();
    let span = error.primary.as_ref().and_then(|label| label.span.as_ref()).unwrap();
    assert_eq!((span.file.as_str(), span.start_line), (TESTS, 3));
}

#[tokio::test]
async fn check_refuses_unsaved_text_at_a_path_the_editor_does_not_open() {
    let (_dir, api) = project();
    for path in ["../escape.test.yaml", ".env", ".vibranium/x.api.yaml", "notes.txt"] {
        let request = wire::CheckRequest {
            overlay: [(path.to_owned(), "x: 1\n".to_owned())].into(),
            ..wire::CheckRequest::default()
        };
        let refused = check(api.clone(), request).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{path}");
        assert!(!refused.message.contains("x: 1"), "the text is never echoed: {}", refused.message);
    }
}

#[tokio::test]
async fn read_then_write_round_trips_and_a_stale_save_is_aborted() {
    let (_dir, api) = project();
    let opened = read_file(api.clone(), wire::ReadFileRequest { path: TESTS.into() }).await.unwrap();
    assert_eq!(opened.text, CRUD);
    let edited = CRUD.replace("Lists every user", "Lists all users");
    let save = wire::WriteFileRequest { path: TESTS.into(), text: edited, expected_digest: Some(opened.digest) };
    let saved = write_file(api.clone(), save.clone()).await.unwrap();
    assert!(saved.diagnostics.iter().all(|found| found.severity() != wire::Severity::Error), "{saved:?}");
    assert!(saved.check_error.is_empty(), "{saved:?}");
    let stale = write_file(api, save).await.unwrap_err();
    assert_eq!(stale.code, Code::Aborted);
}

#[tokio::test]
async fn the_editor_never_opens_env_files_or_hidden_folders() {
    let (_dir, api) = project();
    let schema = read_file(api.clone(), wire::ReadFileRequest { path: ".env.schema".into() }).await.unwrap();
    assert!(!schema.text.contains(SECRET), "the schema names secrets and opens for Settings (D247)");
    for path in [".env", ".env.local", "services/.env.schema", ".vibranium/history.db", ".git/config"] {
        let refused = read_file(api.clone(), wire::ReadFileRequest { path: path.into() }).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{path}");
        assert!(!format!("{refused:?}").contains(SECRET));
    }
    let write = wire::WriteFileRequest { path: ".env".into(), text: "X=1\n".into(), expected_digest: None };
    assert_eq!(write_file(api, write).await.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test]
async fn the_editor_never_writes_a_path_discovery_leaves_out() {
    let (dir, api) = project();
    let write = wire::WriteFileRequest { path: "node_modules/x.test.yaml".into(), ..wire::WriteFileRequest::default() };
    let refused = write_file(api, write).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument);
    assert!(refused.message.contains("help: pick a path the project reads"), "{refused:?}");
    assert!(!dir.path().join("node_modules").exists(), "nothing is created");
}

#[tokio::test]
async fn explain_config_never_sends_a_secret_in_any_layer() {
    let (_dir, api) = project();
    let request = wire::ExplainConfigRequest {
        keys: vec!["vars.note".into(), "vars.SHOP_TOKEN".into()],
        sets: vec![format!("vars.note=again {SECRET}"), format!("vars.SHOP_TOKEN={SECRET}")],
        ..wire::ExplainConfigRequest::default()
    };
    let found = explain_config(api, request).await.unwrap();
    let note = found.values.iter().find(|value| value.key == "vars.note").unwrap();
    assert_eq!(note.layers.len(), 2, "{found:?}");
    let bytes = found.encode_to_vec();
    assert!(!bytes.windows(SECRET.len()).any(|window| window == SECRET.as_bytes()));
    assert!(!format!("{found:?}").contains(SECRET));
}

#[tokio::test]
async fn explain_config_refuses_an_unknown_key_with_help() {
    let (_dir, api) = project();
    let request =
        wire::ExplainConfigRequest { keys: vec!["defaults.timout".into()], ..wire::ExplainConfigRequest::default() };
    let refused = explain_config(api, request).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument);
    assert!(refused.message.contains("help: did you mean `defaults.timeout`"), "{refused:?}");
}

#[tokio::test]
async fn check_secrets_says_each_declared_secret_resolves_and_never_sends_its_value() {
    let (dir, api) = project();
    std::fs::write(dir.path().join(".env.schema"), "SHOP_TOKEN=\nSHOP_CLIENT_ID=\n").unwrap();
    let request = wire::CheckSecretsRequest { profiles: vec!["bare".into()], ..wire::CheckSecretsRequest::default() };
    let found = check_secrets(api, request).await.unwrap();
    let states: Vec<(&str, bool)> = found.secrets.iter().map(|check| (check.name.as_str(), check.resolved)).collect();
    assert_eq!(states, [("SHOP_TOKEN", true), ("SHOP_CLIENT_ID", false)]);
    assert_eq!((found.provider.as_str(), found.error.as_deref()), ("env", None));
    let bytes = found.encode_to_vec();
    assert!(!bytes.windows(SECRET.len()).any(|window| window == SECRET.as_bytes()));
}

#[tokio::test]
async fn check_secrets_refuses_an_unknown_profile() {
    let (_dir, api) = project();
    let request = wire::CheckSecretsRequest { profiles: vec!["prod".into()], ..wire::CheckSecretsRequest::default() };
    assert_eq!(check_secrets(api, request).await.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test]
async fn test_auth_names_the_credentials_of_a_secret_token_and_never_sends_the_token() {
    let (dir, _) = project();
    let config = format!("{CONFIG}auth:\n  - id: shop\n    type: bearer\n    token: ${{secret.SHOP_TOKEN}}\n");
    std::fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    std::fs::write(
        dir.path().join(API),
        USERS.replace("    path: /users\n  - id: create", "    path: /users\n    auth: shop\n  - id: create"),
    )
    .unwrap();
    let engine = vc_engine::EngineBuilder::for_project(dir.path()).unwrap().build().unwrap();
    let api = std::sync::Arc::new(crate::EngineApi::new(std::sync::Arc::new(engine), Vec::new(), Vec::new()));
    let request = wire::TestAuthRequest { auth: "shop".into(), ..wire::TestAuthRequest::default() };
    let tested = test_auth(api, request).await.unwrap();
    assert_eq!(
        (tested.api.as_str(), tested.headers.as_slice()),
        ("users#list", ["authorization".to_owned()].as_slice())
    );
    assert_eq!((tested.expires_in_ms, tested.error.as_deref()), (None, None));
    let bytes = tested.encode_to_vec();
    assert!(!bytes.windows(SECRET.len()).any(|window| window == SECRET.as_bytes()));
}

#[tokio::test]
async fn test_auth_refuses_an_unknown_auth() {
    let (_dir, api) = project();
    let request = wire::TestAuthRequest { auth: "nope".into(), ..wire::TestAuthRequest::default() };
    assert_eq!(test_auth(api, request).await.unwrap_err().code, Code::InvalidArgument);
}
