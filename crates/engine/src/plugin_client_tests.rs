use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_adapter_plugins::FsPluginStore;
use vc_adapter_system::FsSources;
use vc_interfaces::{PluginStderr, SourceError, SourceWriter};
use vc_model::{ContentHash, SourcePath};
use vc_testkit::{FakeVcs, MemoryCompileCache, MemoryEnvironment};

use super::registry_spec;
use crate::sample::plugin_engine;
use crate::{Engine, EngineBuilder, EngineError, PluginState};

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";
const MANIFEST: &str = concat!(
    "id: report\nversion: 1.0.0\nkind: reporter\n",
    "entry: {command: python3, args: [report.py]}\n",
    "permissions: {secrets: [reportToken]}\n",
);

/// A project with a plugin folder `plugin/`, and its engine with the standard plugin store.
fn project() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    fs::create_dir_all(dir.path().join("plugin")).unwrap();
    fs::write(dir.path().join("plugin/vibranium-plugin.yaml"), MANIFEST).unwrap();
    fs::write(dir.path().join("plugin/report.py"), "print()\n").unwrap();
    let engine = plugin_engine(dir.path(), PluginStderr::Inherit);
    (dir, engine)
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap_or_default()
}

/// Whether nothing is left in the staging folder.
fn nothing_staged(dir: &Path) -> bool {
    fs::read_dir(dir.join(".vibranium/plugins/.staged")).map_or(true, |mut entries| entries.next().is_none())
}

/// Checks there is no lock, no installed plugin and nothing staged.
fn untouched(dir: &Path) {
    assert!(!dir.join("vibranium.lock").exists());
    assert!(!dir.join(".vibranium/plugins/report").exists());
    assert!(nothing_staged(dir));
}

fn shown(error: &EngineError) -> String {
    format!("{error} {}", error.help().unwrap_or_default())
}

#[test]
fn a_preview_writes_nothing_and_the_write_installs_with_its_digest() {
    let (dir, engine) = project();
    let preview = engine.client_plugin_add("path:plugin", None).unwrap();
    let manifest = preview.manifest.as_ref().unwrap();
    assert_eq!(
        (manifest.id.as_str(), manifest.permissions.secrets.as_slice()),
        ("report", &["reportToken".to_owned()][..])
    );
    let paths: Vec<&str> = preview.changes.files.iter().map(|change| change.path.as_str()).collect();
    assert_eq!(paths, ["vibranium.config.yaml", "vibranium.lock"]);
    assert_eq!(read(dir.path(), "vibranium.config.yaml"), CONFIG);
    untouched(dir.path());

    let written = engine.client_plugin_add("path:plugin", Some(&preview.digest)).unwrap();
    assert_eq!(written.digest, preview.digest);
    assert!(read(dir.path(), "vibranium.config.yaml").ends_with("  - id: report\n    source: path:plugin\n"));
    assert!(read(dir.path(), "vibranium.lock").contains("secrets: [reportToken]"));
    assert!(dir.path().join(".vibranium/plugins/report/report.py").is_file());
    assert!(nothing_staged(dir.path()));
    let listed = engine.plugin_list().unwrap();
    assert_eq!((listed[0].id.as_str(), &listed[0].state), ("report", &PluginState::Ready));
}

#[test]
fn a_write_after_the_files_or_the_plugin_changed_is_stale_and_installs_nothing() {
    let (dir, engine) = project();
    let preview = engine.client_plugin_add("path:plugin", None).unwrap();
    fs::write(
        dir.path().join("plugin/vibranium-plugin.yaml"),
        MANIFEST.replace("[reportToken]", "[reportToken, apiKey]"),
    )
    .unwrap();
    let error = engine.client_plugin_add("path:plugin", Some(&preview.digest)).unwrap_err();
    assert!(matches!(error, EngineError::Stale { .. }), "{error:?}");
    assert!(!dir.path().join(".vibranium/plugins/report").exists());
    assert!(nothing_staged(dir.path()));

    fs::write(dir.path().join("plugin/vibranium-plugin.yaml"), MANIFEST).unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{CONFIG}# changed\n")).unwrap();
    let error = engine.client_plugin_add("path:plugin", Some(&preview.digest)).unwrap_err();
    assert!(matches!(error, EngineError::Stale { .. }), "{error:?}");
    assert!(!dir.path().join("vibranium.lock").exists());
}

/// A writer that another edit beats: it changes the config just before each unchanged write,
/// after the engine compared the digest it read.
struct Raced(FsSources);

impl SourceWriter for Raced {
    fn write(&self, path: &SourcePath, text: &str) -> Result<(), SourceError> {
        self.0.write(path, text)
    }

    fn write_unchanged(&self, path: &SourcePath, text: &str, was: Option<ContentHash>) -> Result<(), SourceError> {
        self.0.write(&SourcePath::new("vibranium.config.yaml").unwrap(), &format!("{CONFIG}# raced\n"))?;
        self.0.write_unchanged(path, text, was)
    }

    fn remove(&self, path: &SourcePath) -> Result<(), SourceError> {
        self.0.remove(path)
    }

    fn trash(&self, path: &SourcePath, was: ContentHash) -> Result<String, SourceError> {
        self.0.trash(path, was)
    }
}

#[test]
fn a_write_that_fails_after_the_digest_matched_leaves_no_install() {
    let (dir, _) = project();
    let root = dir.path();
    let engine = EngineBuilder::new(root.to_path_buf())
        .vcs(Arc::new(FakeVcs::default()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .sources(Arc::new(FsSources::new(root).unwrap()))
        .writer(Arc::new(Raced(FsSources::new(root).unwrap())))
        .plugins(Arc::new(FsPluginStore::new(root)))
        .build()
        .unwrap();
    let preview = engine.client_plugin_add("path:plugin", None).unwrap();
    let error = engine.client_plugin_add("path:plugin", Some(&preview.digest)).unwrap_err();
    assert!(matches!(error, EngineError::Stale { .. }), "{error:?}");
    assert!(!root.join(".vibranium/plugins/report").exists(), "the plugin installed for the write is taken out");
    assert!(!root.join("vibranium.lock").exists());
    assert!(nothing_staged(root));
}

#[test]
fn a_source_a_client_may_not_name_is_refused_without_echoing_it() {
    let (dir, engine) = project();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("vibranium-plugin.yaml"), MANIFEST).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), dir.path().join("sekrit-link")).unwrap();
    let sources = [
        "path:sekrit-link",
        "path:../sekrit",
        "path:/etc",
        "path:plugin/../../sekrit",
        "npm:file:../sekrit",
        "npm:git+https://user:sekrit@example.com/r.git",
        "npm:github:acme/sekrit",
        "npm:../sekrit",
        "npm:sekrit@file:../x",
        "npm:@sekrit",
        "npm:Sekrit",
        "npm:-sekrit",
        "https://example.com/sekrit.tgz",
    ];
    for source in sources {
        let error = engine.client_plugin_add(source, None).unwrap_err();
        let text = shown(&error);
        assert!(!text.contains(source) && !text.contains("sekrit"), "{source}: {text}");
        assert!(matches!(error, EngineError::Edit { .. } | EngineError::Reporter { .. }), "{source}: {error:?}");
    }
    assert_eq!(read(dir.path(), "vibranium.config.yaml"), CONFIG);
    assert!(nothing_staged(dir.path()));
}

#[test]
fn a_registry_package_and_range_is_all_an_npm_source_may_be() {
    for spec in
        ["left-pad", "@acme/vibranium-report", "@acme/vibranium-report@^2.1.0", "x@>=1 <2", "a@latest", "b@1.x || 2"]
    {
        assert!(registry_spec(spec), "{spec}");
    }
    for spec in ["", "@acme", "@acme/", "Left", ".x", "_x", "a/b", "a@", "a@b:c", "a@../b", "a@b#c", "a@b@c", "a b"] {
        assert!(!registry_spec(spec), "{spec}");
    }
}

#[test]
fn a_wrong_manifest_or_missing_folder_says_why_without_the_source() {
    let (dir, engine) = project();
    fs::create_dir_all(dir.path().join("broken")).unwrap();
    fs::write(dir.path().join("broken/vibranium-plugin.yaml"), "id: html-report\nversion: 1\nkind: reporter\n")
        .unwrap();
    let error = engine.client_plugin_add("path:broken", None).unwrap_err();
    assert_eq!(
        error.to_string(),
        "its vibranium-plugin.yaml: its id `html-report` is not a camelCase word, such as `htmlReport`"
    );
    let error = engine.client_plugin_add("path:missing", None).unwrap_err();
    assert_eq!(error.to_string(), "the plugin could not be fetched");
    assert!(nothing_staged(dir.path()));
}

#[test]
fn another_source_for_the_same_id_is_refused_without_the_one_sent() {
    let (dir, engine) = project();
    let preview = engine.client_plugin_add("path:plugin", None).unwrap();
    engine.client_plugin_add("path:plugin", Some(&preview.digest)).unwrap();
    fs::create_dir_all(dir.path().join("other")).unwrap();
    fs::write(dir.path().join("other/vibranium-plugin.yaml"), MANIFEST).unwrap();
    let text = shown(&engine.client_plugin_add("path:other", None).unwrap_err());
    assert!(text.contains("the config has the plugin `report` from `path:plugin` already"), "{text}");
    assert!(text.contains("to take it from that source instead"), "{text}");
    assert!(!text.contains("path:other"), "{text}");
    assert!(nothing_staged(dir.path()));
}

#[test]
fn a_removal_is_previewed_then_written_and_uninstalled_with_its_digest() {
    let (dir, engine) = project();
    let preview = engine.client_plugin_add("path:plugin", None).unwrap();
    engine.client_plugin_add("path:plugin", Some(&preview.digest)).unwrap();
    let removal = engine.client_plugin_remove("report", None).unwrap();
    assert_eq!(
        removal.changes.titles,
        ["Remove plugin report from the config", "Drop what report may reach from vibranium.lock"]
    );
    assert!(dir.path().join(".vibranium/plugins/report").exists());
    assert!(
        engine.client_plugin_remove("report", Some("0")).is_err_and(|error| matches!(error, EngineError::Stale { .. }))
    );
    assert!(dir.path().join(".vibranium/plugins/report").exists());

    engine.client_plugin_remove("report", Some(&removal.digest)).unwrap();
    assert_eq!(read(dir.path(), "vibranium.config.yaml"), CONFIG);
    assert!(!read(dir.path(), "vibranium.lock").contains("report"));
    assert!(!dir.path().join(".vibranium/plugins/report").exists());
}

#[test]
fn a_removal_of_no_plugin_or_not_an_id_echoes_nothing() {
    let (_dir, engine) = project();
    for id in ["../sekrit", "sekrit/x", "Sekrit-x", "sekritPlugin"] {
        let text = shown(&engine.client_plugin_remove(id, None).unwrap_err());
        assert!(!text.contains("sekrit"), "{id}: {text}");
    }
    let error = engine.client_plugin_remove("sekritPlugin", None).unwrap_err();
    assert_eq!(error.to_string(), "there is no plugin by that ID");
}
