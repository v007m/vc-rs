use std::fs;
use std::sync::Arc;

use vc_adapter_system::FsSources;
use vc_executor::MASKED;
use vc_testkit::{FakeVcs, MemoryEnvironment};

use super::{EditOp, PointerEdit, PointerEdits};
use crate::sample::builder;
use crate::{Engine, EngineError};

/// The syntax reference's project config, with comments the edits must keep.
const CONFIG: &str = "\
# vibranium.config.yaml
syntaxVersion: 2
project: shop

defaults:
  timeout: 10s  # per request
  concurrency: 32

vars:
  usersUrl: http://localhost:8080

profiles:
  - id: dev
    default: true
  # The shared staging stack.
  - id: staging
    vars:
      usersUrl: https://users.staging.shop.example
";
const CONFIG_PATH: &str = "vibranium.config.yaml";
const API: &str = "services/users/users.api.yaml";
const USERS: &str =
    "baseUrl: ${var.usersUrl}\napis:\n  - id: list\n    path: /users\n  - id: get\n    path: /users/1\n";
const TESTS: &str = "services/users/crud.test.yaml";
const CRUD: &str = "tests:\n  - id: listsUsers\n    call: users#list\n";

fn project() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    for (path, text) in [
        (CONFIG_PATH, CONFIG),
        (API, USERS),
        (TESTS, CRUD),
        (".env", "SHOP_TOKEN=tok-edit-31f\n"),
        (".env.schema", "SHOP_TOKEN=\n"),
    ] {
        fs::write(dir.path().join(path), text).unwrap();
    }
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let environment = Arc::new(MemoryEnvironment::new(&[("SHOP_TOKEN", "tok-edit-31f")]));
    let engine = builder(FakeVcs::default()).sources(folder.clone()).writer(folder).environment(environment);
    (dir, engine.build().unwrap())
}

fn edit(pointer: &str, op: EditOp, value: &str) -> PointerEdit {
    PointerEdit { pointer: pointer.into(), op, value: value.into() }
}

fn edits(engine: &Engine, path: &str, edits: Vec<PointerEdit>) -> PointerEdits {
    let digest = engine.read_file(path).unwrap().digest;
    PointerEdits { path: path.into(), digest, edits, write: false }
}

#[tokio::test]
async fn a_set_replaces_a_value_in_place_keeping_comments_and_writes_nothing_until_asked() {
    let (dir, engine) = project();
    let request = edits(&engine, CONFIG_PATH, vec![edit("/defaults/timeout", EditOp::Set, "\"5s\"")]);
    let edited = engine.edit_by_pointer(&request).await.unwrap();
    assert_eq!(edited.text, CONFIG.replace("timeout: 10s  # per request", "timeout: 5s  # per request"));
    assert!(edited.diff.contains("-  timeout: 10s  # per request\n+  timeout: 5s  # per request\n"), "{}", edited.diff);
    assert_eq!((edited.digest.as_str(), edited.check.is_none()), (request.digest.as_str(), true));
    assert_eq!(fs::read_to_string(dir.path().join(CONFIG_PATH)).unwrap(), CONFIG);
}

#[tokio::test]
async fn a_write_saves_the_file_and_returns_its_new_digest_and_check() {
    let (dir, engine) = project();
    let mut request = edits(&engine, CONFIG_PATH, vec![edit("/defaults/concurrency", EditOp::Set, "8")]);
    request.write = true;
    let edited = engine.edit_by_pointer(&request).await.unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(CONFIG_PATH)).unwrap(), CONFIG.replace("32", "8"));
    assert_eq!(edited.digest, engine.read_file(CONFIG_PATH).unwrap().digest);
    assert!(!edited.check.unwrap().unwrap().diagnostics.has_errors());
}

#[tokio::test]
async fn a_set_adds_a_missing_key_with_the_maps_above_it() {
    let (_dir, engine) = project();
    let request = edits(
        &engine,
        CONFIG_PATH,
        vec![
            edit("/defaults/perHost", EditOp::Set, "4"),
            edit("/reports/html/layout", EditOp::Set, "\"single\""),
            edit("/profiles/0/default", EditOp::Set, "false"),
        ],
    );
    let text = engine.edit_by_pointer(&request).await.unwrap().text;
    assert!(text.contains("  concurrency: 32\n  perHost: 4\n"), "{text}");
    assert!(text.ends_with("reports:\n  html:\n    layout: single\n"), "{text}");
    assert!(text.contains("  - id: dev\n    default: false\n"), "{text}");
}

#[tokio::test]
async fn inserts_and_removes_list_items_and_keys_one_after_another() {
    let (_dir, engine) = project();
    let request = edits(
        &engine,
        CONFIG_PATH,
        vec![
            edit(
                "/profiles/-",
                EditOp::Insert,
                r#"{"id": "qa", "vars": {"usersUrl": "https://users.qa.shop.example"}}"#,
            ),
            edit("/profiles/1", EditOp::Remove, ""),
            edit("/vars/ordersUrl", EditOp::Insert, "\"http://localhost:8081\""),
            edit("/defaults/timeout", EditOp::Remove, ""),
        ],
    );
    let text = engine.edit_by_pointer(&request).await.unwrap().text;
    let expected = "# vibranium.config.yaml\nsyntaxVersion: 2\nproject: shop\n\ndefaults:\n  concurrency: 32\n\n\
        vars:\n  usersUrl: http://localhost:8080\n  ordersUrl: http://localhost:8081\n\n\
        profiles:\n  - id: dev\n    default: true\n  - id: qa\n    vars:\n      usersUrl: https://users.qa.shop.example\n";
    assert_eq!(text, expected);
}

#[tokio::test]
async fn a_move_takes_a_list_item_with_its_comment_to_another_place() {
    let (_dir, engine) = project();
    let request = edits(&engine, CONFIG_PATH, vec![edit("/profiles/1", EditOp::Move, "0")]);
    let edited = engine.edit_by_pointer(&request).await.unwrap();
    let staging = "  # The shared staging stack.\n  - id: staging\n    vars:\n      usersUrl: https://users.staging.shop.example\n";
    let (head, _) = CONFIG.split_once("  - id: dev").unwrap();
    assert_eq!(edited.text, format!("{head}{staging}  - id: dev\n    default: true\n"));
    let twice = vec![edit("/profiles/1", EditOp::Move, "0"), edit("/profiles/1", EditOp::Move, "0")];
    assert_eq!(engine.edit_by_pointer(&edits(&engine, CONFIG_PATH, twice)).await.unwrap().text, CONFIG);
}

#[tokio::test]
async fn the_diff_masks_a_secret_value_the_file_repeats() {
    let (dir, engine) = project();
    let config = CONFIG.replacen("vars:\n", "vars:\n  note: 'sent tok-edit-31f'\n", 1);
    fs::write(dir.path().join(CONFIG_PATH), config).unwrap();
    let request = edits(&engine, CONFIG_PATH, vec![edit("/vars/usersUrl", EditOp::Set, "\"http://localhost:9090\"")]);
    let edited = engine.edit_by_pointer(&request).await.unwrap();
    assert!(edited.diff.contains(&format!(" note: 'sent {MASKED}'")), "{}", edited.diff);
    assert!(!format!("{:?}", edited.diff).contains("tok-edit-31f"));
}

#[tokio::test]
async fn a_file_changed_since_it_was_read_is_not_edited() {
    let (dir, engine) = project();
    let request = edits(&engine, CONFIG_PATH, vec![edit("/project", EditOp::Set, "\"store\"")]);
    fs::write(dir.path().join(CONFIG_PATH), CONFIG.replace("shop", "market")).unwrap();
    let stale = engine.edit_by_pointer(&PointerEdits { write: true, ..request }).await.unwrap_err();
    assert!(matches!(stale, EngineError::Stale { .. }), "{stale:?}");
    assert!(fs::read_to_string(dir.path().join(CONFIG_PATH)).unwrap().contains("market"));
}

#[tokio::test]
async fn an_edit_that_adds_an_error_is_refused_and_nothing_is_written() {
    let (dir, engine) = project();
    let mut request = edits(&engine, API, vec![edit("/apis/0", EditOp::Remove, "")]);
    request.write = true;
    let refused = engine.edit_by_pointer(&request).await.unwrap_err();
    assert!(matches!(refused, EngineError::Edit { .. }), "{refused:?}");
    assert!(refused.to_string().contains("nothing was written"), "{refused}");
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), USERS);
    let unused = edits(&engine, API, vec![edit("/apis/1", EditOp::Remove, "")]);
    assert!(!engine.edit_by_pointer(&unused).await.unwrap().text.contains("id: get"));
}

#[tokio::test]
async fn an_edit_that_does_not_fit_is_refused_by_its_place_never_its_pointer_or_value() {
    let (_dir, engine) = project();
    let value = "\"tok-edit-31f\"";
    let cases = [
        (edit("/vars/usersUrl/deeper", EditOp::Set, value), "edit 1 points inside a value that is not a map or list"),
        (edit("/profiles/7", EditOp::Insert, value), "edit 1 adds list items only at the end"),
        (edit("/vars/usersUrl", EditOp::Insert, value), "edit 1 adds a key the map has already"),
        (edit("/defaults", EditOp::Set, value), "edit 1 needs a plain or quoted scalar"),
        (edit("/vars/usersUrl", EditOp::Set, "[1, 2]"), "edit 1 sets a string, number or true/false"),
        (edit("/vars/nobody", EditOp::Remove, ""), "edit 1 points at nothing"),
        (edit("vars", EditOp::Set, value), "edit 1 needs a pointer starting with /"),
        (edit("/project", EditOp::Set, "tok-edit-31f"), "edit 1 needs its value as JSON text"),
        (edit("/profiles/7", EditOp::Move, "0"), "edit 1 needs a block list with items on lines of their own"),
        (edit("/profiles/0", EditOp::Move, value), "edit 1 needs the item's new place as a number"),
        (edit("/vars/usersUrl", EditOp::Move, "0"), "edit 1 moves list items only"),
    ];
    for (bad, told) in cases {
        let refused = engine.edit_by_pointer(&edits(&engine, CONFIG_PATH, vec![bad])).await.unwrap_err();
        let shown = format!("{refused} {refused:?}");
        assert!(shown.contains(told), "{shown}");
        assert!(!shown.contains("tok-edit-31f") && !shown.contains("deeper"), "{shown}");
    }
}

#[tokio::test]
async fn only_the_files_the_editor_opens_are_edited() {
    let (dir, engine) = project();
    fs::write(dir.path().join(".env.schema"), "SHOP_TOKEN=\n").unwrap();
    let paths = ["../escape.test.yaml", ".env", ".env.schema", ".git/config", ".vibranium/x.api.yaml", "notes.txt"];
    for path in paths {
        let request = PointerEdits { path: path.into(), digest: String::new(), edits: Vec::new(), write: true };
        let refused = engine.edit_by_pointer(&request).await.unwrap_err();
        assert!(matches!(refused, EngineError::Edit { .. }), "{path}: {refused:?}");
        assert!(!format!("{refused:?}").contains("tok-edit-31f"), "{refused:?}");
    }
    assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), "SHOP_TOKEN=tok-edit-31f\n");
}
