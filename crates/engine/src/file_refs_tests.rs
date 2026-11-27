use std::fs;
use std::sync::Arc;

use vc_adapter_system::FsSources;
use vc_testkit::FakeVcs;

use super::FileReference;
use crate::sample::builder;
use crate::{Engine, EngineError};

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\n";
const USERS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n";
const CRUD: &str = "tests:\n  - id: listsUsers\n    name: Lists users\n    call: users#list\n";
const API: &str = "services/users/users.api.yaml";
const TESTS: &str = "services/users/crud.test.yaml";

fn project() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join(API), USERS).unwrap();
    fs::write(dir.path().join(TESTS), CRUD).unwrap();
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    (dir, builder(FakeVcs::default()).sources(folder.clone()).writer(folder).build().unwrap())
}

#[test]
fn the_references_list_names_each_call_from_another_file_and_the_files_it_could_not_read() {
    let (dir, engine) = project();
    fs::create_dir_all(dir.path().join("services/orders")).unwrap();
    fs::write(dir.path().join("services/orders/broken.test.yaml"), "tests: [\n").unwrap();
    let found = engine.file_references(API).unwrap();
    let reference = FileReference {
        path: TESTS.into(),
        line: 4,
        from: "users.crud#listsUsers".into(),
        target: "users#list".into(),
    };
    assert_eq!(found.references, vec![reference]);
    assert_eq!(found.unread, vec!["services/orders/broken.test.yaml".to_owned()]);
    assert_eq!(engine.file_references(TESTS).unwrap().references, Vec::new());
    let config = engine.file_references("vibranium.config.yaml");
    assert!(matches!(config, Err(EngineError::Edit { .. })), "{config:?}");
    let missing = engine.file_references("services/users/none.test.yaml");
    assert!(matches!(missing, Err(EngineError::NoFilesMatch { .. })), "{missing:?}");
}

#[test]
fn a_trashed_file_moves_under_the_project_trash_only_while_it_is_the_file_opened() {
    let (dir, engine) = project();
    let opened = engine.read_file(TESTS).unwrap();
    let stale = engine.trash_file(TESTS, "0");
    assert!(matches!(stale, Err(EngineError::Stale { .. })), "{stale:?}");
    let place = engine.trash_file(TESTS, &opened.digest).unwrap();
    assert!(place.starts_with(".vibranium/trash/") && place.ends_with(&format!("/{TESTS}")), "{place}");
    assert!(!dir.path().join(TESTS).exists());
    assert_eq!(fs::read_to_string(dir.path().join(&place)).unwrap(), CRUD);
    assert_eq!(engine.file_references(API).unwrap(), super::FileReferences::default(), "the trash is not discovered");
    let config = engine.read_file("vibranium.config.yaml").unwrap();
    let refused = engine.trash_file("vibranium.config.yaml", &config.digest);
    assert!(matches!(refused, Err(EngineError::Edit { .. })), "{refused:?}");
    assert!(dir.path().join("vibranium.config.yaml").exists());
}

#[test]
fn the_editor_text_formats_in_the_house_style_without_being_written() {
    let (dir, engine) = project();
    let text = "apis:\n  - path: /people\n    id: list\nbaseUrl: ${var.baseUrl}\n";
    let formatted = engine.format_text(API, text, false).unwrap();
    assert_eq!(formatted, "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /people\n");
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), USERS);
    let broken = engine.format_text(API, "apis: [\n", false);
    assert!(matches!(broken, Err(EngineError::Edit { .. })), "{broken:?}");
    let hidden = engine.format_text(".env", "A=1\n", false);
    assert!(matches!(hidden, Err(EngineError::Edit { .. })), "{hidden:?}");
}
