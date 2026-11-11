use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_adapter_plugins::FsPluginStore;
use vc_adapter_system::FsSources;
use vc_diagnostics::{Code, Severity};
use vc_testkit::{FakeVcs, MemoryCompileCache, MemoryEnvironment};

use crate::{CheckRequest, Engine, EngineBuilder};

const CONFIG: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n";

/// A project with a reporter plugin in `report/` and an auth plugin in `stamp/`, and its engine.
fn project() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), CONFIG).unwrap();
    plugin(dir.path(), "report", "kind: reporter\npermissions: {secrets: [reportToken]}\n");
    plugin(dir.path(), "stamp", "kind: auth\npermissions: {}\n");
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let engine = EngineBuilder::new(dir.path().to_path_buf())
        .vcs(Arc::new(FakeVcs::default()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .sources(folder.clone())
        .writer(folder)
        .plugins(Arc::new(FsPluginStore::new(dir.path())))
        .build()
        .unwrap();
    (dir, engine)
}

fn plugin(root: &Path, id: &str, rest: &str) {
    fs::create_dir_all(root.join(id)).unwrap();
    let manifest = format!("id: {id}\nversion: 1.0.0\nentry: {{command: python3, args: [main.py]}}\n{rest}");
    fs::write(root.join(id).join("vibranium-plugin.yaml"), manifest).unwrap();
    fs::write(root.join(id).join("main.py"), "print()\n").unwrap();
}

fn added(engine: &Engine, source: &str) {
    let offer = engine.plugin_fetch(source).unwrap();
    engine.write(&engine.plugin_approve(&offer).unwrap()).unwrap();
}

/// Each diagnostic `vc check` gives, as its code, severity, line and message.
fn checked(engine: &Engine) -> Vec<(Code, Severity, u32, String)> {
    let report = engine.check(&CheckRequest::default()).unwrap();
    let line = |diagnostic: &vc_diagnostics::Diagnostic| diagnostic.primary.span.start.line;
    report.diagnostics.iter().map(|found| (found.code, found.severity, line(found), found.message.clone())).collect()
}

fn config_with(dir: &Path, more: &str) {
    let path = dir.join("vibranium.config.yaml");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{text}{more}")).unwrap();
}

#[test]
fn a_plugin_the_lock_does_not_approve_is_a_warning_at_its_id() {
    let (dir, engine) = project();
    config_with(dir.path(), "plugins:\n  - id: report\n    source: path:report\n");
    let found = checked(&engine);
    assert_eq!(found.len(), 1, "{found:?}");
    let (code, severity, line, message) = &found[0];
    assert_eq!((*code, *severity, *line), (Code::V5120, Severity::Warning, 6));
    assert_eq!(message, "a run cannot use plugin `report`: not approved: `vc plugin add` it");
}

#[test]
fn an_added_plugin_is_clean_until_it_asks_for_more_or_its_config_is_templated() {
    let (dir, engine) = project();
    added(&engine, "path:report");
    assert_eq!(checked(&engine), Vec::new());
    let installed = dir.path().join(".vibranium/plugins/report/vibranium-plugin.yaml");
    let manifest = fs::read_to_string(&installed).unwrap().replace("[reportToken]", "[reportToken, apiKey]");
    fs::write(&installed, manifest).unwrap();
    let found = checked(&engine);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].3.ends_with("asks for more: secret apiKey; `vc plugin add` it again"), "{found:?}");
    added(&engine, "path:report");
    config_with(dir.path(), "vars:\n  reportUrl: https://reports.example.com\n");
    let text = fs::read_to_string(dir.path().join("vibranium.config.yaml")).unwrap();
    let templated =
        text.replace("source: path:report\n", "source: path:report\n    config: {url: \"${var.reportUrl}\"}\n");
    fs::write(dir.path().join("vibranium.config.yaml"), templated).unwrap();
    let found = checked(&engine);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, Code::V5120);
    assert!(found[0].3.contains("its `config`: a plugin's `config` takes literal values"), "{found:?}");
}

#[test]
fn an_auth_plugin_named_as_a_report_format_is_v2302() {
    let (dir, engine) = project();
    added(&engine, "path:report");
    added(&engine, "path:stamp");
    config_with(dir.path(), "reports:\n  formats: [ctrf, report, stamp]\n");
    let found = checked(&engine);
    assert_eq!(found.len(), 1, "{found:?}");
    let (code, severity, _, message) = &found[0];
    assert_eq!((*code, *severity), (Code::V2302, Severity::Error));
    assert_eq!(message, "`stamp` is an `auth` plugin, not a reporter, so it writes no report");
}

#[test]
fn a_broken_or_unreadable_installed_manifest_is_a_warning_and_no_plugin_code_runs() {
    let (dir, engine) = project();
    fs::write(dir.path().join("report/main.py"), "open('ran', 'w')\n").unwrap();
    added(&engine, "path:report");
    let installed = dir.path().join(".vibranium/plugins/report/vibranium-plugin.yaml");
    fs::write(&installed, "id: [not, a, manifest\n").unwrap();
    let found = checked(&engine);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].0, found[0].1), (Code::V5120, Severity::Warning));
    assert!(found[0].3.ends_with("not installed: `vc plugin add` it again"), "{found:?}");
    fs::remove_file(&installed).unwrap();
    fs::create_dir(&installed).unwrap();
    let found = checked(&engine);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, Code::V5120, "{found:?}");
    assert!(found[0].3.starts_with("a run cannot use plugin `report`: "), "{found:?}");
    assert!(!dir.path().join("ran").exists() && !dir.path().join(".vibranium/plugins/report/ran").exists());
}
