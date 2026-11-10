use prost::Message as _;
use vc_contracts::engine::v1 as wire;

use super::{check, explain_config, get_project, list_definitions, read_file, write_file};
use crate::Code;
use crate::engine_fixture::{CRUD, SECRET, TESTS, project};

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
async fn read_then_write_round_trips_and_a_stale_save_is_aborted() {
    let (_dir, api) = project();
    let opened = read_file(api.clone(), wire::ReadFileRequest { path: TESTS.into() }).await.unwrap();
    assert_eq!(opened.text, CRUD);
    let edited = CRUD.replace("Lists every user", "Lists all users");
    let save = wire::WriteFileRequest { path: TESTS.into(), text: edited, expected_digest: Some(opened.digest) };
    let saved = write_file(api.clone(), save.clone()).await.unwrap();
    assert!(saved.diagnostics.iter().all(|found| found.severity() != wire::Severity::Error), "{saved:?}");
    let stale = write_file(api, save).await.unwrap_err();
    assert_eq!(stale.code, Code::Aborted);
}

#[tokio::test]
async fn the_editor_never_opens_env_files_or_hidden_folders() {
    let (_dir, api) = project();
    for path in [".env", ".env.schema", ".vibranium/history.db", ".git/config"] {
        let refused = read_file(api.clone(), wire::ReadFileRequest { path: path.into() }).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{path}");
        assert!(!format!("{refused:?}").contains(SECRET));
    }
    let write = wire::WriteFileRequest { path: ".env".into(), text: "X=1\n".into(), expected_digest: None };
    assert_eq!(write_file(api, write).await.unwrap_err().code, Code::InvalidArgument);
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
