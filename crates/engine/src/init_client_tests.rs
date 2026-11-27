use std::fs;
use std::path::Path;

use super::InitChoices;
use crate::{CheckRequest, Engine, EngineBuilder, EngineError};

/// An empty folder and an engine rooted at it, as `vc ui` starts with no project.
fn folder() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let engine = EngineBuilder::new(dir.path().to_path_buf()).build().unwrap();
    (dir, engine)
}

fn wizard() -> InitChoices {
    InitChoices {
        name: Some("Shop: v2".to_owned()),
        service: Some("users".to_owned()),
        base_url: Some("https://api.example.com/v1".to_owned()),
        profile: Some("dev".to_owned()),
        sample: false,
    }
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap_or_default()
}

/// The paths of the files in `dir`, sorted, without the engine's own `.vibranium/`.
fn listed(dir: &Path) -> Vec<String> {
    let mut paths: Vec<String> = walkdir(dir, dir);
    paths.retain(|path| !path.starts_with(".vibranium/"));
    paths.sort();
    paths
}

fn walkdir(root: &Path, dir: &Path) -> Vec<String> {
    let entries = fs::read_dir(dir).map(|entries| entries.flatten().collect::<Vec<_>>()).unwrap_or_default();
    entries
        .iter()
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                walkdir(root, &path)
            } else {
                vec![path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")]
            }
        })
        .collect()
}

fn errors(engine: &Engine) -> usize {
    engine.check(&CheckRequest::default()).unwrap().summary().errors
}

#[test]
fn the_wizard_previews_its_project_then_writes_it_with_the_digest() {
    let (dir, engine) = folder();
    let preview = engine.client_init(&wizard(), None).unwrap();
    let paths: Vec<&str> = preview.changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, [".env.schema", ".gitignore", "services/users/users.api.yaml", "vibranium.config.yaml"]);
    assert_eq!(listed(dir.path()), Vec::<String>::new());
    assert_ne!(preview.root, "");

    let written = engine.client_init(&wizard(), Some(&preview.digest)).unwrap();
    assert_eq!(written.digest, preview.digest);
    assert_eq!(listed(dir.path()), paths);
    the_wizard_wrote_its_choices(dir.path());
    assert_eq!(errors(&engine), 0);

    let again = engine.client_init(&wizard(), None).unwrap_err();
    assert_eq!(again.to_string(), "services/users/users.api.yaml already exists");
}

/// The config and api file hold the wizard's choices.
fn the_wizard_wrote_its_choices(dir: &Path) {
    let config = read(dir, "vibranium.config.yaml");
    assert!(config.contains("project: 'Shop: v2'\n") || config.contains("project: \"Shop: v2\"\n"), "{config}");
    assert!(config.contains("\nvars:\n  baseUrl: https://api.example.com/v1\n"), "{config}");
    assert!(config.contains("profiles:\n  - id: dev\n    default: true\n"), "{config}");
    assert!(!config.contains("sample"), "{config}");
    let api = read(dir, "services/users/users.api.yaml");
    assert!(api.contains("baseUrl: ${var.baseUrl}\n"), "{api}");
}

#[test]
fn a_write_with_other_choices_or_after_the_folder_changed_is_stale_and_writes_nothing() {
    let (dir, engine) = folder();
    let preview = engine.client_init(&wizard(), None).unwrap();
    let other = InitChoices { name: Some("Other".to_owned()), ..wizard() };
    let error = engine.client_init(&other, Some(&preview.digest)).unwrap_err();
    assert!(matches!(error, EngineError::Stale { .. }), "{error:?}");
    assert_eq!(listed(dir.path()), Vec::<String>::new());

    fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
    let error = engine.client_init(&wizard(), Some(&preview.digest)).unwrap_err();
    assert!(matches!(error, EngineError::Stale { .. }), "{error:?}");
    assert_eq!(listed(dir.path()), [".gitignore"]);
    let fresh = engine.client_init(&wizard(), None).unwrap();
    engine.client_init(&wizard(), Some(&fresh.digest)).unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "target/\n.vibranium/\n.env\n");
}

#[test]
fn a_choice_that_does_not_fit_is_refused_without_echoing_it() {
    let (dir, engine) = folder();
    let bad = [
        InitChoices { name: Some("sekrit\nx".to_owned()), ..wizard() },
        InitChoices { name: Some(" ".to_owned()), ..wizard() },
        InitChoices { name: Some("sekrit".repeat(20)), ..wizard() },
        InitChoices { profile: Some("Sekrit".to_owned()), ..wizard() },
        InitChoices { profile: Some("sekrit-dev".to_owned()), ..wizard() },
        InitChoices { service: Some("../sekrit".to_owned()), ..wizard() },
        InitChoices { service: Some("sekrit.x".to_owned()), ..wizard() },
        InitChoices { service: Some("/sekrit".to_owned()), ..wizard() },
        InitChoices { base_url: Some("https://user:sekrit@example.com".to_owned()), ..wizard() },
        InitChoices { base_url: Some("ftp://sekrit.example".to_owned()), ..wizard() },
        InitChoices { base_url: Some("https://${secret.sekrit}".to_owned()), ..wizard() },
        InitChoices { base_url: Some("https://sekrit.example/a b".to_owned()), ..wizard() },
        InitChoices { base_url: Some("http://".to_owned()), ..wizard() },
        InitChoices { service: Some("sample".to_owned()), sample: true, ..wizard() },
    ];
    for choices in bad {
        let error = engine.client_init(&choices, None).unwrap_err();
        let text = format!("{error} {}", error.help().unwrap_or_default());
        assert!(matches!(error, EngineError::Edit { .. }), "{choices:?}: {error:?}");
        assert!(!text.contains("sekrit") && text.starts_with("the new project: "), "{choices:?}: {text}");
    }
    assert_eq!(listed(dir.path()), Vec::<String>::new());
}

#[test]
fn the_sample_and_a_base_url_share_one_vars_map() {
    let (dir, engine) = folder();
    let choices = InitChoices { sample: true, ..wizard() };
    let preview = engine.client_init(&choices, None).unwrap();
    engine.client_init(&choices, Some(&preview.digest)).unwrap();
    let config = read(dir.path(), "vibranium.config.yaml");
    assert!(config.contains("\nvars:\n  sampleUrl: http://localhost:7421 "), "{config}");
    assert!(config.contains("services/sample\n  baseUrl: https://api.example.com/v1\n\nprofiles:"), "{config}");
    assert!(dir.path().join("services/sample/basics.test.yaml").is_file());
    assert_eq!(errors(&engine), 0);
}

#[test]
fn the_demo_writes_the_sample_project_once() {
    let (dir, engine) = folder();
    let preview = engine.client_demo(None).unwrap();
    let paths: Vec<&str> = preview.changes.files.iter().map(|file| file.path.as_str()).collect();
    assert!(paths.contains(&"services/sample/sample.api.yaml") && paths.contains(&"vibranium.config.yaml"));
    assert_eq!(listed(dir.path()), Vec::<String>::new());
    engine.client_demo(Some(&preview.digest)).unwrap();
    assert!(read(dir.path(), "vibranium.config.yaml").contains("sampleUrl: http://localhost:7421"));
    assert_eq!(errors(&engine), 0);

    let again = engine.client_demo(None).unwrap();
    assert!(again.changes.files.is_empty(), "{again:?}");
    engine.client_demo(Some(&again.digest)).unwrap();
}

#[test]
fn a_project_says_whether_its_config_is_there() {
    let (dir, engine) = folder();
    assert!(!engine.project_info().unwrap().configured);
    fs::write(dir.path().join("vibranium.config.yaml"), "syntaxVersion: 2\nprofiles:\n  - id: dev\n").unwrap();
    assert!(engine.project_info().unwrap().configured);
}
