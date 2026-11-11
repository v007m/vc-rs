use std::fs;
use std::sync::Arc;

use vc_contracts::engine::v1 as wire;

use super::edit_file;
use crate::Code;
use crate::engine_api::EngineApi;
use crate::engine_fixture::{API, CONFIG, SECRET, USERS, project};

const CONFIG_PATH: &str = "vibranium.config.yaml";

fn set(pointer: &str, value_json: &str) -> wire::PointerEdit {
    wire::PointerEdit { pointer: pointer.into(), op: wire::EditOp::Set.into(), value_json: value_json.into() }
}

fn request(path: &str, digest: String, edits: Vec<wire::PointerEdit>, write: bool) -> wire::EditFileRequest {
    wire::EditFileRequest { path: path.into(), digest, edits, write }
}

fn digest_of(api: &Arc<EngineApi>, path: &str) -> String {
    api.engine().read_file(path).unwrap().digest
}

fn leaks(response: &impl prost::Message) -> bool {
    let bytes = response.encode_to_vec();
    bytes.windows(SECRET.len()).any(|window| window == SECRET.as_bytes())
}

#[tokio::test]
async fn a_preview_shows_the_masked_diff_then_a_write_saves_and_checks_the_file() {
    let (dir, api) = project();
    let digest = digest_of(&api, CONFIG_PATH);
    let edits = vec![set("/vars/baseUrl", "\"http://127.0.0.1:10\"")];
    let preview = edit_file(api.clone(), request(CONFIG_PATH, digest.clone(), edits.clone(), false)).await.unwrap();
    assert!(preview.diff.contains("+  baseUrl: 'http://127.0.0.1:10'\n"), "{}", preview.diff);
    assert!(preview.diff.contains("  note: 'sent ••••'"), "the context line is masked: {}", preview.diff);
    assert_eq!((preview.written, preview.digest.as_str()), (false, digest.as_str()));
    assert!(!leaks(&preview) && !format!("{preview:?}").contains(SECRET));
    assert_eq!(fs::read_to_string(dir.path().join(CONFIG_PATH)).unwrap(), CONFIG);
    let written = edit_file(api.clone(), request(CONFIG_PATH, digest, edits, true)).await.unwrap();
    assert!(written.written && written.check_error.is_empty(), "{written:?}");
    assert!(written.diagnostics.iter().all(|found| found.severity() != wire::Severity::Error), "{written:?}");
    assert_eq!(written.digest, digest_of(&api, CONFIG_PATH));
    assert!(!leaks(&written));
}

#[tokio::test]
async fn an_edit_of_a_file_changed_since_it_was_read_is_aborted() {
    let (_dir, api) = project();
    let digest = digest_of(&api, CONFIG_PATH);
    let edits = vec![set("/project", "\"store\"")];
    edit_file(api.clone(), request(CONFIG_PATH, digest.clone(), edits.clone(), true)).await.unwrap();
    let stale = edit_file(api, request(CONFIG_PATH, digest, edits, true)).await.unwrap_err();
    assert_eq!(stale.code, Code::Aborted, "{stale:?}");
}

#[tokio::test]
async fn an_edit_that_adds_an_error_or_does_not_fit_is_refused_without_its_value() {
    let (dir, api) = project();
    let digest = digest_of(&api, API);
    let remove = wire::PointerEdit { pointer: "/apis/0".into(), op: wire::EditOp::Remove.into(), ..Default::default() };
    let refused = edit_file(api.clone(), request(API, digest, vec![remove], true)).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument, "the test calls users#list: {refused:?}");
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), USERS);
    let digest = digest_of(&api, CONFIG_PATH);
    let quoted = format!("\"{SECRET}\"");
    let no_op = wire::PointerEdit { pointer: "/vars/x".into(), value_json: quoted.clone(), ..Default::default() };
    for (edit, told) in [(set("/vars/note/inner", &quoted), "edit 1 points inside"), (no_op, "edit 1 needs an op")] {
        let refused =
            edit_file(api.clone(), request(CONFIG_PATH, digest.clone(), vec![edit], false)).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
        assert!(refused.message.contains(told), "{refused:?}");
        assert!(!format!("{refused:?}").contains(SECRET) && !refused.message.contains("inner"), "{refused:?}");
    }
}

#[tokio::test]
async fn edits_never_reach_env_files_hidden_folders_or_paths_outside_the_project() {
    let (dir, api) = project();
    let paths = [
        "../escape.test.yaml",
        "/etc/hosts",
        ".env",
        ".env.local",
        ".env.schema",
        "services/.env.schema",
        ".vibranium/history.db",
        ".git/config",
        "notes.txt",
    ];
    for path in paths {
        let edits = vec![set("/SHOP_TOKEN", "\"x\"")];
        let refused = edit_file(api.clone(), request(path, String::new(), edits, true)).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{path}: {refused:?}");
        assert!(!format!("{refused:?}").contains(SECRET), "{refused:?}");
    }
    assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), format!("SHOP_TOKEN={SECRET}\n"));
    assert!(!dir.path().parent().unwrap().join("escape.test.yaml").exists());
}
