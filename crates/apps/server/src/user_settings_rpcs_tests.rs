use std::fs;
use std::sync::Arc;

use tempfile::TempDir;
use vc_contracts::engine::v1 as wire;
use vc_engine::EngineBuilder;

use super::{read_user_settings, write_user_settings};
use crate::engine_fixture::{CONFIG, SECRET};
use crate::{Code, EngineApi};

/// A project with `CONFIG`, and an API whose engine keeps its user config at the returned path.
fn project() -> (TempDir, std::path::PathBuf, Arc<EngineApi>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::write(dir.path().join(".env.schema"), "SHOP_TOKEN=\n").unwrap();
    fs::write(dir.path().join(".env"), format!("SHOP_TOKEN={SECRET}\n")).unwrap();
    let user = dir.path().join("home/config.yaml");
    let engine = EngineBuilder::for_project(dir.path()).unwrap().user_config(user.clone()).build().unwrap();
    (dir, user, Arc::new(EngineApi::new(Arc::new(engine), Vec::new(), Vec::new())))
}

fn setting(key: &str, value: Option<&str>) -> wire::UserSettingEdit {
    wire::UserSettingEdit { key: key.into(), value: value.map(str::to_owned) }
}

#[tokio::test]
async fn a_setting_is_previewed_then_written_with_the_digest_then_read_back_masked() {
    let (_dir, user, api) = project();
    fs::create_dir_all(user.parent().unwrap()).unwrap();
    fs::write(&user, format!("vars:\n  token: {SECRET}\n")).unwrap();
    let edits = vec![setting("defaults.timeout", Some("12s")), setting("profile", Some("bare"))];
    let request = wire::WriteUserSettingsRequest { edits: edits.clone(), ..Default::default() };
    let preview = write_user_settings(api.clone(), request).await.unwrap();
    assert!(!format!("{preview:?}").contains(SECRET), "{preview:?}");
    assert!(preview.after.ends_with("defaults:\n  timeout: 12s\nprofile: bare\n"), "{preview:?}");
    let request = wire::WriteUserSettingsRequest { edits, write: true, expected_digest: Some(preview.digest) };
    let written = write_user_settings(api.clone(), request).await.unwrap();
    assert!(fs::read_to_string(&user).unwrap().contains(SECRET), "the file keeps the value; only the answer masks it");
    let read = read_user_settings(api, wire::ReadUserSettingsRequest {}).await.unwrap();
    assert_eq!(read.digest, written.digest);
    let keys: Vec<(&str, bool)> =
        read.settings.iter().map(|setting| (setting.key.as_str(), setting.shadowed)).collect();
    assert_eq!(keys, [("vars.token", false), ("defaults.timeout", false), ("profile", true)], "`dev` is the default");
    assert!(!format!("{read:?}").contains(SECRET), "{read:?}");
}

#[tokio::test]
async fn a_write_without_the_digest_is_aborted_and_a_bad_key_is_an_invalid_argument() {
    let (_dir, user, api) = project();
    let request = wire::WriteUserSettingsRequest {
        edits: vec![setting("profile", Some("dev"))],
        write: true,
        expected_digest: None,
    };
    assert_eq!(write_user_settings(api.clone(), request).await.unwrap_err().code, Code::Aborted);
    for edit in [setting("plugins.allowUnconfined", Some("true")), setting("reports.dir", Some("../out"))] {
        let request = wire::WriteUserSettingsRequest { edits: vec![edit], ..Default::default() };
        assert_eq!(write_user_settings(api.clone(), request).await.unwrap_err().code, Code::InvalidArgument);
    }
    assert!(!user.exists());
}
