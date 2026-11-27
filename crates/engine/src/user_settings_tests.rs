use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use vc_testkit::MemoryEnvironment;

use super::{NEW_FILE, UserSetting, UserSettingEdit};
use crate::{Engine, EngineBuilder, EngineError};

const SECRET: &str = "tok-user-41c9";
const CONFIG: &str = "syntaxVersion: 2\ndefaults:\n  timeout: 5s\nprofiles:\n  - id: dev\n  - id: qa\n";

struct Fixture {
    _dirs: (tempfile::TempDir, tempfile::TempDir),
    user: PathBuf,
    engine: Engine,
}

fn fixture() -> Fixture {
    let (project, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    fs::write(project.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::write(project.path().join(".env.schema"), "SHOP_TOKEN=\n").unwrap();
    fs::write(project.path().join(".env"), format!("SHOP_TOKEN={SECRET}\n")).unwrap();
    let user = home.path().join(".config/vibranium/config.yaml");
    let environment = MemoryEnvironment::new(&[("HOME", &home.path().display().to_string())]);
    let engine = EngineBuilder::new(project.path().to_path_buf())
        .environment(Arc::new(environment))
        .user_config(user.clone())
        .build()
        .unwrap();
    Fixture { _dirs: (project, home), user, engine }
}

fn set(key: &str, value: &str) -> UserSettingEdit {
    UserSettingEdit { key: key.into(), value: Some(value.into()) }
}

fn setting(key: &str, value: &str, shadowed: bool) -> UserSetting {
    UserSetting { key: key.into(), value: value.into(), shadowed }
}

#[tokio::test]
async fn a_missing_user_config_is_shown_under_home_with_no_settings() {
    let fixture = fixture();
    let read = fixture.engine.user_settings().await.unwrap();
    assert_eq!(read.path, "~/.config/vibranium/config.yaml");
    assert_eq!((read.digest.as_str(), read.entries.as_slice()), (NEW_FILE, [].as_slice()));
}

#[tokio::test]
async fn a_change_is_previewed_then_written_with_its_digest_then_read_back() {
    let fixture = fixture();
    let edits = [set("defaults.timeout", "12s"), set("profile", "qa"), set("vars.owner", "sam")];
    let preview = fixture.engine.user_settings_write(&edits, None).await.unwrap();
    assert_eq!((preview.before.as_deref(), preview.digest.as_str()), (None, NEW_FILE));
    assert!(preview.after.ends_with("defaults:\n  timeout: 12s\nprofile: qa\nvars:\n  owner: sam\n"), "{preview:?}");
    assert!(!fixture.user.exists(), "a preview writes nothing");
    let written = fixture.engine.user_settings_write(&edits, Some(&preview.digest)).await.unwrap();
    assert_eq!(fs::read_to_string(&fixture.user).unwrap(), preview.after);
    let read = fixture.engine.user_settings().await.unwrap();
    assert_eq!(read.digest, written.digest);
    let expected = [
        setting("defaults.timeout", "12s", true),
        setting("profile", "qa", false),
        setting("vars.owner", "sam", false),
    ];
    assert_eq!(read.entries, expected, "the project sets defaults.timeout, so the user's does not count here");
    let removing = [UserSettingEdit { key: "defaults.timeout".into(), value: None }];
    let preview = fixture.engine.user_settings_write(&removing, None).await.unwrap();
    fixture.engine.user_settings_write(&removing, Some(&preview.digest)).await.unwrap();
    assert!(fs::read_to_string(&fixture.user).unwrap().ends_with("\nprofile: qa\nvars:\n  owner: sam\n"));
}

#[tokio::test]
async fn a_write_without_the_previews_digest_or_after_the_file_changed_is_stale() {
    let fixture = fixture();
    let edits = [set("defaults.perHost", "2")];
    let refused = fixture.engine.user_settings_write(&edits, Some("")).await.unwrap_err();
    assert!(matches!(refused, EngineError::Stale { .. }), "{refused:?}");
    let preview = fixture.engine.user_settings_write(&edits, None).await.unwrap();
    fs::create_dir_all(fixture.user.parent().unwrap()).unwrap();
    fs::write(&fixture.user, "profile: dev\n").unwrap();
    let stale = fixture.engine.user_settings_write(&edits, Some(&preview.digest)).await.unwrap_err();
    assert!(matches!(stale, EngineError::Stale { .. }), "{stale:?}");
    assert_eq!(fs::read_to_string(&fixture.user).unwrap(), "profile: dev\n");
}

#[tokio::test]
async fn a_key_or_value_the_user_config_does_not_take_is_refused_without_echoing_it() {
    let fixture = fixture();
    let refusals = [
        set("plugins.allowUnconfined", "true"),
        set("auth.sekrit", "x"),
        set("vars.sekrit-name", "x"),
        set("defaults.timeout", "sekrit"),
        set("defaults.concurrency", "0"),
        set("reports.html.layout", "sekrit"),
        set("profile", "sekrit"),
        set("reports.dir", "../sekrit"),
        set("reports.dir", "/tmp/sekrit"),
    ];
    for edit in refusals {
        let refused = fixture.engine.user_settings_write(std::slice::from_ref(&edit), None).await.unwrap_err();
        let told = format!("{refused} {:?}", refused.help());
        assert!(!told.contains("sekrit"), "{edit:?}: {told}");
        assert!(matches!(refused, EngineError::Setting { .. } | EngineError::Reporter { .. }), "{edit:?}: {refused:?}");
    }
    assert!(!fixture.user.exists());
}

#[tokio::test]
async fn a_secrets_value_in_the_user_config_is_masked_in_the_read_and_the_preview() {
    let fixture = fixture();
    fs::create_dir_all(fixture.user.parent().unwrap()).unwrap();
    fs::write(&fixture.user, format!("vars:\n  token: {SECRET}\n")).unwrap();
    let read = fixture.engine.user_settings().await.unwrap();
    assert!(!format!("{read:?}").contains(SECRET), "{read:?}");
    let preview = fixture.engine.user_settings_write(&[set("profile", "dev")], None).await.unwrap();
    assert!(!format!("{preview:?}").contains(SECRET), "{preview:?}");
}
