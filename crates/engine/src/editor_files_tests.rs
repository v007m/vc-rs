use std::fs;
use std::sync::Arc;

use vc_adapter_system::FsSources;
use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider, SourceWriter};
use vc_model::{ContentHash, SourcePath};
use vc_testkit::FakeVcs;

use super::SaveFile;
use crate::sample::builder;
use crate::{Engine, EngineError};

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n";
const USERS: &str = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n";
const API: &str = "services/users/users.api.yaml";
const NEW: &str = "services/users/crud.test.yaml";
const BROKEN: &str = "tests:\n  - id: listsUsers\n    call: users#nothing\n";

fn project() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::write(dir.path().join(".env"), "TOKEN=abc\n").unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join(API), USERS).unwrap();
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    (dir, builder(FakeVcs::default()).sources(folder.clone()).writer(folder).build().unwrap())
}

fn save(path: &str, text: &str, expected_digest: Option<&str>) -> SaveFile {
    SaveFile { path: path.into(), text: text.into(), expected_digest: expected_digest.map(str::to_owned) }
}

#[test]
fn a_file_opens_with_its_digest_and_saves_over_it_while_unchanged() {
    let (dir, engine) = project();
    let opened = engine.read_file(API).unwrap();
    assert_eq!((opened.text.as_str(), opened.digest.len()), (USERS, 64));
    let text = USERS.replace("/users", "/people");
    let saved = engine.save_file(&save(API, &text, Some(&opened.digest))).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), text);
    assert_eq!(saved.digest, engine.read_file(API).unwrap().digest);
    assert!(!saved.check.unwrap().diagnostics.has_errors());
}

/// A project whose files read, but whose search fails, so every check after a save fails.
struct Unsearchable(FsSources);

impl SourceProvider for Unsearchable {
    fn discover(&self, _filter: &SourceFilter) -> Result<Discovery, SourceError> {
        Err(SourceError::Root { root: ".".into(), reason: "the disk went away".into() })
    }

    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError> {
        self.0.read(paths)
    }
}

#[test]
fn a_save_whose_check_fails_still_returns_the_digest_it_wrote() {
    let (dir, _) = project();
    let sources = Arc::new(Unsearchable(FsSources::new(dir.path()).unwrap()));
    let writer = Arc::new(FsSources::new(dir.path()).unwrap());
    let engine = builder(FakeVcs::default()).sources(sources).writer(writer).build().unwrap();
    let opened = engine.read_file(API).unwrap();
    let text = USERS.replace("/users", "/people");
    let saved = engine.save_file(&save(API, &text, Some(&opened.digest))).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), text);
    assert_eq!(saved.digest, engine.read_file(API).unwrap().digest);
    assert!(matches!(saved.check, Err(EngineError::Sources { .. })), "{:?}", saved.check);
    let again = engine.save_file(&save(API, USERS, Some(&saved.digest)));
    assert!(again.is_ok(), "{again:?}");
}

#[test]
fn a_save_keeps_text_with_errors_and_returns_them() {
    let (dir, engine) = project();
    let saved = engine.save_file(&save(NEW, BROKEN, None)).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(NEW)).unwrap(), BROKEN);
    assert!(saved.check.unwrap().diagnostics.has_errors());
}

#[test]
fn a_save_over_a_file_that_changed_since_it_was_opened_writes_nothing() {
    let (dir, engine) = project();
    let opened = engine.read_file(API).unwrap();
    fs::write(dir.path().join(API), "apis: []\n").unwrap();
    let stale = engine.save_file(&save(API, USERS, Some(&opened.digest)));
    let Err(error @ EngineError::Stale { .. }) = stale else { panic!("{stale:?}") };
    assert!(error.to_string().contains("changed since it was opened") && error.help().is_some(), "{error}");
    let exists = engine.save_file(&save(API, USERS, None));
    assert!(matches!(&exists, Err(EngineError::Stale { reason }) if reason.contains("exists already")), "{exists:?}");
    let removed = engine.save_file(&save(NEW, USERS, Some(&opened.digest)));
    assert!(matches!(&removed, Err(EngineError::Stale { reason }) if reason.contains("removed")), "{removed:?}");
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), "apis: []\n");
}

/// A writer that another writer races: it writes `OTHER` to the file just before each unchanged
/// write, after the engine compared the digest it read.
struct Raced(FsSources);

const OTHER: &str = "apis: []\n";

impl SourceWriter for Raced {
    fn write(&self, path: &SourcePath, text: &str) -> Result<(), SourceError> {
        self.0.write(path, text)
    }

    fn write_unchanged(&self, path: &SourcePath, text: &str, was: Option<ContentHash>) -> Result<(), SourceError> {
        self.0.write(path, OTHER)?;
        self.0.write_unchanged(path, text, was)
    }

    fn remove(&self, path: &SourcePath) -> Result<(), SourceError> {
        self.0.remove(path)
    }
}

#[test]
fn a_save_over_a_file_changed_after_its_digest_was_compared_keeps_the_other_text() {
    let (dir, _) = project();
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let raced = Arc::new(Raced(FsSources::new(dir.path()).unwrap()));
    let engine = builder(FakeVcs::default()).sources(folder).writer(raced).build().unwrap();
    let opened = engine.read_file(API).unwrap();
    let stale = engine.save_file(&save(API, &USERS.replace("/users", "/people"), Some(&opened.digest)));
    assert!(matches!(&stale, Err(EngineError::Stale { reason }) if reason.contains("changed since")), "{stale:?}");
    assert_eq!(fs::read_to_string(dir.path().join(API)).unwrap(), OTHER);
    let created = engine.save_file(&save(NEW, BROKEN, None));
    assert!(matches!(&created, Err(EngineError::Stale { .. })), "{created:?}");
    assert_eq!(fs::read_to_string(dir.path().join(NEW)).unwrap(), OTHER);
}

#[test]
fn only_the_config_and_definition_files_outside_hidden_folders_open() {
    let (_dir, engine) = project();
    assert!(engine.read_file("vibranium.config.yaml").unwrap().text.contains("project: shop"));
    let refused = [
        ".env",
        ".ENV",
        "../x.test.yaml",
        "/etc/x.test.yaml",
        "services\\..\\..\\x.test.yaml",
        "C:\\x.test.yaml",
        "\\\\host\\share\\x.test.yaml",
        ".vibranium/a.test.yaml",
        ".Vibranium/a.test.yaml",
        "services/.git/a.api.yaml",
        "services\\.git\\a.api.yaml",
        "notes.txt",
    ];
    for path in refused {
        let read = engine.read_file(path);
        assert!(matches!(read, Err(EngineError::Edit { .. })), "{path}: {read:?}");
        let written = engine.save_file(&save(path, "x", None));
        assert!(matches!(written, Err(EngineError::Edit { .. })), "{path}: {written:?}");
    }
    let missing = engine.read_file(NEW);
    assert!(matches!(missing, Err(EngineError::NoFilesMatch { .. })), "{missing:?}");
}

#[test]
fn a_path_discovery_leaves_out_is_neither_read_nor_written() {
    let (dir, engine) = project();
    fs::write(dir.path().join(".gitignore"), "generated/\n").unwrap();
    fs::create_dir_all(dir.path().join("generated")).unwrap();
    fs::write(dir.path().join("generated/old.api.yaml"), USERS).unwrap();
    for path in ["node_modules/x.test.yaml", "generated/old.api.yaml", "generated/new.test.yaml"] {
        assert_left_out(&engine, path);
    }
    assert!(!dir.path().join("node_modules").exists(), "no folder is created");
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{CONFIG}paths:\n  exclude: [legacy]\n")).unwrap();
    assert_left_out(&engine, "legacy/a.test.yaml");
    assert!(!dir.path().join("legacy").exists());
    assert!(engine.read_file("vibranium.config.yaml").is_ok(), "the config always opens");
}

/// `path` is neither read nor written, and the refusal says what to do.
fn assert_left_out(engine: &Engine, path: &str) {
    let read = engine.read_file(path);
    assert!(matches!(&read, Err(EngineError::Edit { help: Some(_), .. })), "{path}: {read:?}");
    let written = engine.save_file(&save(path, "tests: []\n", None));
    assert!(matches!(written, Err(EngineError::Edit { .. })), "{path}: {written:?}");
}

#[test]
fn the_project_shows_its_name_profiles_and_default() {
    let (_dir, engine) = project();
    let info = engine.project_info().unwrap();
    assert_eq!((info.name.as_str(), info.vc_version.as_str()), ("shop", env!("CARGO_PKG_VERSION")));
    assert_eq!((info.profiles, info.defaults), (vec!["dev".to_owned(), "staging".to_owned()], vec!["dev".to_owned()]));
    assert_eq!(crate::sample::engine(FakeVcs::default()).project_info().unwrap().name, "project");
}

/// Links in `dir`'s project, beside its files: one to `outside`'s `secret.test.yaml`, one to the
/// project's `.env`. Their project paths.
#[cfg(unix)]
fn links(dir: &tempfile::TempDir, outside: &tempfile::TempDir) -> [&'static str; 2] {
    fs::write(outside.path().join("secret.test.yaml"), "kept\n").unwrap();
    let escape = "services/users/escape.test.yaml";
    std::os::unix::fs::symlink(outside.path().join("secret.test.yaml"), dir.path().join(escape)).unwrap();
    let env = "services/users/env.test.yaml";
    std::os::unix::fs::symlink(dir.path().join(".env"), dir.path().join(env)).unwrap();
    [escape, env]
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_project_or_to_a_hidden_file_is_neither_read_nor_written() {
    let (dir, engine) = project();
    let outside = tempfile::tempdir().unwrap();
    for link in links(&dir, &outside) {
        let read = engine.read_file(link);
        assert!(
            matches!(&read, Err(EngineError::Edit { reason, .. }) if reason.contains("links to a file outside")),
            "{read:?}"
        );
        let written = engine.save_file(&save(link, "changed\n", None));
        assert!(matches!(written, Err(EngineError::Edit { .. })), "{written:?}");
    }
    assert_eq!(fs::read_to_string(outside.path().join("secret.test.yaml")).unwrap(), "kept\n");
    assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), "TOKEN=abc\n");
}

#[cfg(unix)]
#[test]
fn a_new_file_under_a_link_to_a_left_out_folder_is_refused_and_creates_nothing() {
    let (dir, engine) = project();
    fs::write(dir.path().join(".gitignore"), "node_modules/\n").unwrap();
    fs::create_dir_all(dir.path().join("node_modules")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("node_modules"), dir.path().join("vendor")).unwrap();
    for path in ["vendor/new.test.yaml", "vendor/deep/new.test.yaml"] {
        assert_left_out(&engine, path);
    }
    let made = fs::read_dir(dir.path().join("node_modules")).unwrap().count();
    assert_eq!(made, 0, "nothing is written through the link");
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_project_or_to_a_hidden_file_is_not_checked_as_unsaved_text() {
    let (dir, engine) = project();
    let outside = tempfile::tempdir().unwrap();
    for link in links(&dir, &outside) {
        let overlay = [(link.to_owned(), "tests: []\n".to_owned())].into();
        let checked = engine.check(&crate::CheckRequest { overlay, ..crate::CheckRequest::default() });
        assert!(matches!(checked, Err(EngineError::Edit { .. })), "{link}: {checked:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_read_or_is_not_utf8_is_an_error_and_is_never_saved_over() {
    use std::os::unix::fs::PermissionsExt as _;
    let (dir, engine) = project();
    let (locked, binary) = ("services/users/locked.test.yaml", "services/users/binary.test.yaml");
    fs::write(dir.path().join(locked), "kept\n").unwrap();
    fs::write(dir.path().join(binary), b"\xff\xfe\n").unwrap();
    fs::set_permissions(dir.path().join(locked), fs::Permissions::from_mode(0o000)).unwrap();
    let results = [locked, binary].map(|path| (engine.read_file(path), engine.save_file(&save(path, "new\n", None))));
    fs::set_permissions(dir.path().join(locked), fs::Permissions::from_mode(0o600)).unwrap();
    for (read, saved) in results {
        assert!(matches!(&read, Err(EngineError::Sources { .. })), "{read:?}");
        assert!(matches!(&saved, Err(EngineError::Sources { .. })), "{saved:?}");
    }
    assert_eq!(fs::read_to_string(dir.path().join(locked)).unwrap(), "kept\n");
    assert_eq!(fs::read(dir.path().join(binary)).unwrap(), b"\xff\xfe\n");
    let missing = engine.read_file("services/users/missing.test.yaml");
    assert!(matches!(missing, Err(EngineError::NoFilesMatch { .. })), "{missing:?}");
}
