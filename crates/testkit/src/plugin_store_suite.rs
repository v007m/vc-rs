use vc_interfaces::{Fetched, PluginStore, PluginStoreError};

/// Builds a store over a fresh project holding these files: project-relative paths and their text.
pub type MakePluginStore<'a> = dyn FnMut(&[(&str, &str)]) -> Box<dyn PluginStore> + 'a;

/// The manifest of the plugin folder `plugin/` the suite fetches.
const MANIFEST: &str = "id: suite\nversion: 1.0.0\nkind: reporter\nentry: {command: python3}\n";

/// A project with a plugin folder `plugin/`.
const PROJECT: &[(&str, &str)] = &[("plugin/vibranium-plugin.yaml", MANIFEST), ("plugin/report.py", "print()\n")];

/// The contract every [`PluginStore`] meets (D112). `make` builds a store over a project with
/// the given files.
pub fn plugin_store_suite(make: &mut MakePluginStore<'_>) {
    installs_reads_back_and_uninstalls(make(PROJECT).as_ref());
    a_discarded_plugin_is_never_installed(make(PROJECT).as_ref());
    a_folder_without_a_manifest_is_not_fetched(make);
    reads_the_lock_when_there(make);
}

fn fetch(store: &dyn PluginStore, source: &str) -> Option<Fetched> {
    let fetched = store.fetch(source);
    assert_eq!(fetched.as_ref().err(), None, "fetch failed");
    fetched.ok()
}

fn manifest(store: &dyn PluginStore, id: &str) -> Option<String> {
    let read = store.manifest(id);
    assert_eq!(read.as_ref().err(), None, "manifest failed");
    read.unwrap_or_default()
}

fn installs_reads_back_and_uninstalls(store: &dyn PluginStore) {
    let Some(fetched) = fetch(store, "path:plugin") else { return };
    assert_eq!(fetched.manifest, MANIFEST);
    assert_eq!(manifest(store, "suite"), None, "not installed until approved");
    assert_eq!(store.integrity("suite"), Ok(None), "nothing installed has no integrity");
    assert_eq!(store.install(&fetched, "suite"), Ok(()));
    assert_eq!(manifest(store, "suite").as_deref(), Some(MANIFEST));
    assert_eq!(store.integrity("suite"), Ok(Some(fetched.integrity)), "the installed files hash as fetched");
    uninstalls(store);
}

fn uninstalls(store: &dyn PluginStore) {
    assert_eq!(store.uninstall("suite"), Ok(()));
    assert_eq!(manifest(store, "suite"), None);
    assert_eq!(store.integrity("suite"), Ok(None));
    assert_eq!(store.uninstall("suite"), Ok(()), "nothing to remove is not an error");
}

fn a_discarded_plugin_is_never_installed(store: &dyn PluginStore) {
    let Some(fetched) = fetch(store, "path:plugin") else { return };
    store.discard(&fetched);
    store.discard(&fetched);
    assert_eq!(manifest(store, "suite"), None);
    assert!(matches!(store.install(&fetched, "../x"), Err(PluginStoreError::Install { .. })));
}

fn a_folder_without_a_manifest_is_not_fetched(make: &mut MakePluginStore<'_>) {
    let store = make(&[("plugin/report.py", "print()\n")]);
    assert!(matches!(store.fetch("path:plugin"), Err(PluginStoreError::Fetch { .. })));
}

fn reads_the_lock_when_there(make: &mut MakePluginStore<'_>) {
    assert_eq!(make(&[]).lock(), Ok(None));
    let store = make(&[("vibranium.lock", "lockVersion: 1\n")]);
    assert_eq!(store.lock().ok().flatten().as_deref(), Some("lockVersion: 1\n"));
}
