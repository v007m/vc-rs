use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

use vc_interfaces::{Fetched, PluginStore, PluginStoreError};
use vc_testkit::plugin_store_suite;

use super::FsPluginStore;

const MANIFEST: &str = "id: report\nversion: 1.0.0\nkind: reporter\nentry: {command: python3}\n";

/// A project with a plugin folder `plugin/` holding a manifest and a script.
fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("plugin")).unwrap();
    fs::write(root.path().join("plugin/vibranium-plugin.yaml"), MANIFEST).unwrap();
    fs::write(root.path().join("plugin/report.py"), "print()\n").unwrap();
    root
}

fn staged_count(root: &Path) -> usize {
    fs::read_dir(root.join(".vibranium/plugins/.staged")).map_or(0, Iterator::count)
}

#[test]
fn the_folder_store_meets_the_plugin_store_contract() {
    let projects = tempfile::tempdir().unwrap();
    let mut made = 0;
    plugin_store_suite(&mut |files| {
        made += 1;
        let root = projects.path().join(made.to_string());
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Box::new(FsPluginStore::new(root)) as Box<dyn PluginStore>
    });
}

#[test]
fn a_folder_is_fetched_installed_read_back_and_uninstalled() {
    let root = project();
    let store = FsPluginStore::new(root.path());
    let fetched = store.fetch("path:plugin").unwrap();
    assert_eq!((fetched.manifest.as_str(), fetched.resolved.as_str()), (MANIFEST, "plugin"));
    assert!(fetched.integrity.starts_with("blake3-"));
    assert_eq!(store.manifest("report").unwrap(), None, "not installed until approved");
    store.install(&fetched, "report").unwrap();
    assert_eq!(store.manifest("report").unwrap().as_deref(), Some(MANIFEST));
    assert!(root.path().join(".vibranium/plugins/report/report.py").is_file());
    assert_eq!(staged_count(root.path()), 0);
    store.uninstall("report").unwrap();
    assert_eq!(store.manifest("report").unwrap(), None);
    store.uninstall("report").unwrap();
}

#[test]
fn installing_again_replaces_the_old_folder() {
    let root = project();
    let store = FsPluginStore::new(root.path());
    store.install(&store.fetch("path:plugin").unwrap(), "report").unwrap();
    fs::remove_file(root.path().join("plugin/report.py")).unwrap();
    store.install(&store.fetch("path:plugin").unwrap(), "report").unwrap();
    assert!(!root.path().join(".vibranium/plugins/report/report.py").exists());
}

#[test]
fn a_declined_plugin_is_discarded() {
    let root = project();
    let store = FsPluginStore::new(root.path());
    let fetched = store.fetch("path:plugin").unwrap();
    store.discard(&fetched);
    assert!(!fetched.staged.exists());
    assert_eq!(staged_count(root.path()), 0);
}

#[test]
fn a_folder_without_a_manifest_leaves_nothing_behind() {
    let root = project();
    fs::remove_file(root.path().join("plugin/vibranium-plugin.yaml")).unwrap();
    let store = FsPluginStore::new(root.path());
    let Err(PluginStoreError::Fetch { spec, reason }) = store.fetch("path:plugin") else { panic!("fetched") };
    assert_eq!(spec, "path:plugin");
    assert!(reason.contains("no `vibranium-plugin.yaml`"), "{reason}");
    assert_eq!(staged_count(root.path()), 0);
    assert!(store.fetch("git:x").is_err_and(|error| error.to_string().contains("`npm:` or `path:`")));
    assert!(store.fetch("npm:--global").is_err());
}

#[test]
fn an_id_that_is_not_a_plain_word_or_a_folder_from_elsewhere_is_refused() {
    let root = project();
    let store = FsPluginStore::new(root.path());
    let fetched = store.fetch("path:plugin").unwrap();
    assert!(matches!(store.install(&fetched, "../x"), Err(PluginStoreError::Install { .. })));
    assert!(store.uninstall("..").is_err());
    assert_eq!(store.manifest("../plugin").unwrap(), None);
    let elsewhere = Fetched { staged: root.path().join("plugin"), ..fetched };
    assert!(store.install(&elsewhere, "report").is_err_and(|error| error.to_string().contains("not fetched")));
    store.discard(&elsewhere);
    assert!(root.path().join("plugin").is_dir(), "never removes a folder it did not fetch");
}

#[test]
fn the_lock_is_read_when_there() {
    let root = project();
    let store = FsPluginStore::new(root.path());
    assert_eq!(store.lock().unwrap(), None);
    fs::write(root.path().join("vibranium.lock"), "lockVersion: 1\n").unwrap();
    assert_eq!(store.lock().unwrap().as_deref(), Some("lockVersion: 1\n"));
}

/// What npm 10 says on stderr when the registry has no such package, its log pointer last.
#[cfg(unix)]
const NPM_E404: &str = concat!(
    "npm error code E404\n",
    "npm error 404 Not Found - GET https://registry.npmjs.org/@acme%2fnothing - Not found\n",
    "npm error 404\n",
    "npm error 404  '@acme/nothing@*' is not in this registry.\n",
    "npm error 404\n",
    "npm error 404 Note that you can also install from a\n",
    "npm error 404 tarball, folder, http url, or git repo.\n",
    "npm error A complete log of this run can be found in: /home/a/.npm/_logs/2026-11-11T10_00_00_000Z-debug-0.log\n",
);

/// A fake `npm` that packs a package holding `MANIFEST`, or fails as npm 10 does for a package
/// the registry does not have when `fail` is set.
#[cfg(unix)]
fn fake_npm(root: &Path, fail: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let package = root.join("work/package");
    fs::create_dir_all(&package).unwrap();
    fs::write(package.join("vibranium-plugin.yaml"), MANIFEST).unwrap();
    let script = format!(
        concat!(
            "#!/bin/sh\n",
            "if [ {fail} = 1 ]; then printf '%s' '{e404}' >&2; exit 1; fi\n",
            "tar -czf \"$5/acme-report-2.1.0.tgz\" -C '{work}' package\n",
            "echo '[{{\"name\":\"@acme/report\",\"version\":\"2.1.0\",",
            "\"filename\":\"acme-report-2.1.0.tgz\",\"integrity\":\"sha512-abc\"}}]'\n",
        ),
        fail = u8::from(fail),
        e404 = NPM_E404.replace('\'', "'\\''"),
        work = root.join("work").display(),
    );
    let npm = root.join("npm");
    fs::write(&npm, script).unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    npm
}

#[cfg(unix)]
#[test]
fn an_npm_package_is_packed_and_unpacked_with_npm_s_integrity() {
    let root = project();
    let store = FsPluginStore::new(root.path()).with_npm(fake_npm(root.path(), false));
    let fetched = store.fetch("npm:@acme/report@^2").unwrap();
    assert_eq!((fetched.resolved.as_str(), fetched.integrity.as_str()), ("@acme/report@2.1.0", "sha512-abc"));
    assert_eq!(fetched.manifest, MANIFEST);
    assert!(!fetched.staged.join("acme-report-2.1.0.tgz").exists(), "the tarball is removed");
}

#[cfg(unix)]
#[test]
fn a_failed_npm_shows_its_error_code_and_reason_not_its_log_pointer() {
    let root = project();
    let store = FsPluginStore::new(root.path()).with_npm(fake_npm(root.path(), true));
    let error = store.fetch("npm:@acme/nothing").unwrap_err().to_string();
    let reason = "`npm` failed: E404: 404 Not Found - GET https://registry.npmjs.org/@acme%2fnothing - Not found";
    assert!(error.contains(reason), "{error}");
    assert!(!error.contains("A complete log"), "{error}");
    assert_eq!(staged_count(root.path()), 0);
}
