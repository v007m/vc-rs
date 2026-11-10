use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_adapter_plugins::FsPluginStore;
use vc_adapter_system::FsSources;
use vc_interfaces::{PluginKind, Secrets};
use vc_testkit::FakeVcs;

use super::PluginState;
use crate::plugin_hosts::Hosts;
use crate::sample::builder;
use crate::{Engine, EngineError};

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
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let engine = builder(FakeVcs::default())
        .sources(folder.clone())
        .writer(folder)
        .plugins(Arc::new(FsPluginStore::new(dir.path())))
        .build()
        .unwrap();
    (dir, engine)
}

fn added(engine: &Engine, source: &str) {
    let offer = engine.plugin_fetch(source).unwrap();
    engine.write(&engine.plugin_approve(&offer).unwrap()).unwrap();
}

fn read(dir: &Path, path: &str) -> String {
    fs::read_to_string(dir.join(path)).unwrap_or_default()
}

fn refusal(error: &EngineError) -> (String, Option<String>) {
    let help = error.help();
    (error.to_string(), help)
}

#[test]
fn an_approved_plugin_is_in_the_config_the_lock_and_its_folder() {
    let (dir, engine) = project();
    let offer = engine.plugin_fetch("path:plugin").unwrap();
    assert_eq!((offer.manifest.id.as_str(), offer.manifest.kind), ("report", PluginKind::Reporter));
    assert_eq!(offer.manifest.permissions.secrets, ["reportToken"]);
    let changes = engine.plugin_approve(&offer).unwrap();
    let paths: Vec<&str> = changes.files.iter().map(|change| change.path.as_str()).collect();
    assert_eq!(paths, ["vibranium.config.yaml", "vibranium.lock"]);
    engine.write(&changes).unwrap();
    let config = read(dir.path(), "vibranium.config.yaml");
    assert!(config.contains("plugins:\n  # report · reporter plugin · keys: "), "{config}");
    assert!(config.ends_with("  - id: report\n    source: path:plugin\n"), "{config}");
    let lock = read(dir.path(), "vibranium.lock");
    assert!(lock.contains("  - id: report\n    source: path:plugin\n    resolved: plugin\n"), "{lock}");
    assert!(lock.contains("      secrets: [reportToken]\n"), "{lock}");
    assert!(dir.path().join(".vibranium/plugins/report/report.py").is_file());
    listed_ready(&engine);
}

/// Checks `vc plugin list` shows the one plugin `report`, ready.
fn listed_ready(engine: &Engine) {
    let listed = engine.plugin_list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].id.as_str(), listed[0].source.as_str()), ("report", "path:plugin"));
    assert_eq!(listed[0].state, PluginState::Ready);
    assert_eq!(listed[0].granted.secrets, ["reportToken"]);
}

#[test]
fn approving_again_from_the_same_source_only_updates_the_lock() {
    let (dir, engine) = project();
    added(&engine, "path:plugin");
    fs::write(
        dir.path().join("plugin/vibranium-plugin.yaml"),
        MANIFEST.replace("[reportToken]", "[reportToken, apiKey]"),
    )
    .unwrap();
    let offer = engine.plugin_fetch("path:plugin").unwrap();
    let changes = engine.plugin_approve(&offer).unwrap();
    assert_eq!(changes.files.iter().map(|change| change.path.as_str()).collect::<Vec<_>>(), ["vibranium.lock"]);
    engine.write(&changes).unwrap();
    assert!(read(dir.path(), "vibranium.lock").contains("secrets: [reportToken, apiKey]"));
}

#[test]
fn another_source_for_the_same_id_is_refused_and_dropped() {
    let (dir, engine) = project();
    added(&engine, "path:plugin");
    fs::create_dir_all(dir.path().join("other")).unwrap();
    fs::write(dir.path().join("other/vibranium-plugin.yaml"), MANIFEST).unwrap();
    let offer = engine.plugin_fetch("path:other").unwrap();
    let (reason, help) = refusal(&engine.plugin_approve(&offer).unwrap_err());
    assert_eq!(reason, "the config has the plugin `report` from `path:plugin` already");
    assert_eq!(help.as_deref(), Some("`vc plugin remove report` first to take it from `path:other` instead"));
    assert!(fs::read_dir(dir.path().join(".vibranium/plugins/.staged")).unwrap().next().is_none());
}

#[test]
fn a_declined_plugin_leaves_nothing() {
    let (dir, engine) = project();
    let offer = engine.plugin_fetch("path:plugin").unwrap();
    engine.plugin_decline(&offer);
    assert!(!dir.path().join(".vibranium/plugins/report").exists());
    assert!(fs::read_dir(dir.path().join(".vibranium/plugins/.staged")).unwrap().next().is_none());
    assert_eq!(read(dir.path(), "vibranium.config.yaml"), CONFIG);
    assert_eq!(engine.plugin_list().unwrap(), []);
}

#[test]
fn a_bad_source_or_manifest_is_refused_with_why() {
    let (dir, engine) = project();
    let (reason, help) = refusal(&engine.plugin_fetch("git:acme/report").unwrap_err());
    assert_eq!(reason, "`git:acme/report` is not a plugin source");
    assert_eq!(help.as_deref(), Some("a plugin source is `npm:<package>[@<range>]` or `path:<folder>`"));
    fs::write(dir.path().join("plugin/vibranium-plugin.yaml"), MANIFEST.replace("reporter", "pager")).unwrap();
    let (reason, _) = refusal(&engine.plugin_fetch("path:plugin").unwrap_err());
    assert_eq!(
        reason,
        "the vibranium-plugin.yaml of `path:plugin`: its kind `pager` is none of reporter, auth, importer, executor"
    );
    assert!(fs::read_dir(dir.path().join(".vibranium/plugins/.staged")).unwrap().next().is_none());
    let (reason, _) = refusal(&engine.plugin_fetch("path:nowhere").unwrap_err());
    assert!(reason.starts_with("cannot fetch `path:nowhere`: "), "{reason}");
}

#[test]
fn the_list_says_why_a_run_cannot_use_a_plugin() {
    let (dir, engine) = project();
    added(&engine, "path:plugin");
    let installed = dir.path().join(".vibranium/plugins/report/vibranium-plugin.yaml");
    fs::write(&installed, MANIFEST.replace("[reportToken]", "[reportToken, apiKey]")).unwrap();
    assert_eq!(engine.plugin_list().unwrap()[0].state, PluginState::AsksForMore(vec!["secret apiKey".to_owned()]));
    fs::remove_dir_all(dir.path().join(".vibranium/plugins/report")).unwrap();
    assert_eq!(engine.plugin_list().unwrap()[0].state, PluginState::NotInstalled);
    let lock = read(dir.path(), "vibranium.lock");
    fs::write(dir.path().join("vibranium.lock"), "lockVersion: 1\nplugins: []\n").unwrap();
    assert_eq!(engine.plugin_list().unwrap()[0].state, PluginState::NotApproved);
    fs::write(dir.path().join("vibranium.lock"), lock).unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    assert_eq!(engine.plugin_list().unwrap()[0].state, PluginState::NotInConfig);
    fs::write(dir.path().join("vibranium.lock"), "lockVersion: 9\n").unwrap();
    let (reason, _) = refusal(&engine.plugin_list().unwrap_err());
    assert_eq!(reason, "vibranium.lock: it is lock version 9; this vc reads version 1");
}

#[test]
fn a_removed_plugin_leaves_the_config_the_lock_and_its_folder() {
    let (dir, engine) = project();
    added(&engine, "path:plugin");
    let changes = engine.plugin_remove("report").unwrap();
    assert_eq!(
        changes.titles,
        ["Remove plugin report from the config", "Drop what report may reach from vibranium.lock"]
    );
    engine.write(&changes).unwrap();
    assert_eq!(read(dir.path(), "vibranium.config.yaml").trim_end(), CONFIG.trim_end());
    assert!(read(dir.path(), "vibranium.lock").ends_with("lockVersion: 1\nplugins: []\n"));
    assert!(!dir.path().join(".vibranium/plugins/report").exists());
    let (reason, help) = refusal(&engine.plugin_remove("reprt").unwrap_err());
    assert_eq!((reason.as_str(), help.as_deref()), ("there is no plugin `reprt`", Some("`vc plugin list` lists them")));
}

#[test]
fn an_unknown_plugin_is_refused_with_the_close_one() {
    let (_dir, engine) = project();
    added(&engine, "path:plugin");
    let (reason, help) = refusal(&engine.plugin_remove("reprt").unwrap_err());
    assert_eq!((reason.as_str(), help.as_deref()), ("there is no plugin `reprt`", Some("did you mean `report`?")));
}

/// The hosts a run of the project gets.
fn hosts(engine: &Engine) -> Hosts {
    let (program, _) = engine.program().unwrap();
    engine.plugin_hosts(&program, &Secrets::new())
}

#[test]
fn a_run_gets_a_host_for_each_usable_plugin_and_why_not_for_the_others() {
    let (dir, engine) = project();
    added(&engine, "path:plugin");
    let config = format!("{}  - id: other\n    source: path:other\n", read(dir.path(), "vibranium.config.yaml"));
    fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    let hosts = hosts(&engine);
    let id = |id: &str, kind| hosts.get(id, kind).map(|host| host.manifest().id.clone());
    assert_eq!(id("report", PluginKind::Reporter), Ok("report".to_owned()));
    assert_eq!((hosts.of(PluginKind::Reporter).count(), hosts.of(PluginKind::Auth).count()), (1, 0));
    assert_eq!(id("report", PluginKind::Auth), Err("plugin `report` is a `reporter` plugin, not `auth`".to_owned()));
    let other = "plugin `other`: not approved: `vc plugin add` it";
    assert_eq!(id("other", PluginKind::Reporter), Err(other.to_owned()));
    assert_eq!(hosts.notices(), [other]);
    assert_eq!(id("nope", PluginKind::Reporter), Err("there is no plugin `nope` in the config".to_owned()));
    let more = PluginState::AsksForMore(vec!["secret apiKey".to_owned()]);
    assert_eq!(more.to_string(), "asks for more: secret apiKey; `vc plugin add` it again");
}

#[test]
fn a_plugin_config_takes_literal_values_only() {
    let (dir, engine) = project();
    added(&engine, "path:plugin");
    let (path, config) = (dir.path().join("vibranium.config.yaml"), read(dir.path(), "vibranium.config.yaml"));
    fs::write(&path, format!("{config}    config: {{theme: dark}}\n")).unwrap();
    assert_eq!(hosts(&engine).notices(), Vec::<String>::new());
    fs::write(&path, format!("{config}    config: {{theme: '${{env.HOME}}'}}\n")).unwrap();
    let notices = hosts(&engine).notices();
    let [notice] = notices.as_slice() else { panic!("one notice, not {notices:?}") };
    assert!(notice.starts_with("plugin `report`: its `config`: "), "{notice}");
    assert!(notice.contains("takes literal values; the plugin reads a secret with `secrets/get`"), "{notice}");
}
