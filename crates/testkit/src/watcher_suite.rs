use std::collections::BTreeSet;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use vc_interfaces::FileWatcher;
use vc_model::SourcePath;

/// Writes `text` to a project-relative path in the watched project, as an editor saving it would.
pub type SaveFile<'a> = dyn FnMut(&str, &str) + 'a;

/// How long the suite waits for the saves to be reported.
const WAIT: Duration = Duration::from_secs(10);

/// The saves the suite makes: two definition files, one of them twice, and files the default
/// [`vc_interfaces::SourceFilter`] refuses.
const SAVES: [&str; 5] = [
    "services/users/users.api.yaml",
    "notes.md",
    "services/users/crud.test.yaml",
    ".vibranium/reports/run.json",
    "services/users/users.api.yaml",
];

/// The contract every [`FileWatcher`] meets (D112). `watcher` watches an empty project with the
/// default filter, and `save` writes a file in it.
///
/// Every batch is in path order with each path once, and the batches together report exactly
/// the saved definition files: nothing the filter refuses.
///
/// # Panics
///
/// When `watcher` breaks the contract.
pub fn file_watcher_suite(watcher: &dyn FileWatcher, save: &mut SaveFile<'_>) {
    let batches = watcher.watch();
    assert!(batches.is_ok(), "an existing folder can be watched: {batches:?}");
    let Ok(batches) = batches else { return };
    std::thread::sleep(Duration::from_millis(100));
    for path in SAVES {
        save(path, "tests: []\n");
    }
    let expected = ["services/users/crud.test.yaml", "services/users/users.api.yaml"];
    assert_eq!(reported(&batches, expected.len()), expected, "the saved definition files, and only those");
}

/// The paths of the batches that arrive until `count` paths are reported or the wait ends.
fn reported(batches: &Receiver<Vec<SourcePath>>, count: usize) -> Vec<String> {
    let deadline = Instant::now() + WAIT;
    let mut seen = BTreeSet::new();
    while seen.len() < count {
        let Ok(batch) = batches.recv_timeout(deadline.saturating_duration_since(Instant::now())) else { break };
        assert!(batch.is_sorted_by(|a, b| a < b), "a batch is in path order, each path once: {batch:?}");
        seen.extend(batch.iter().map(|path| path.as_str().to_owned()));
    }
    seen.into_iter().collect()
}
