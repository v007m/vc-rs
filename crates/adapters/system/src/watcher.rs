use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecursiveMode, Watcher as _};
use vc_interfaces::{FileWatcher, SourceFilter, WatchError};
use vc_model::{DEFAULT_SECRETS_SCHEMA, SourcePath};

use crate::config_sources::DOT_ENV;

/// How long the files must be quiet before a burst of saves goes out as one batch (D180).
const DEBOUNCE: Duration = Duration::from_millis(200);

/// The files at the root that are not definition files but that every run reads.
const ROOT_FILES: [&str; 2] = [DOT_ENV, DEFAULT_SECRETS_SCHEMA];

/// The OS's file events, from `notify`.
type Events = Receiver<notify::Result<Event>>;

/// Watches the project folder with the OS's file events (`notify`, D190).
///
/// A batch holds the definition files the watch's filter lets through, and the root `.env` and
/// `.env.schema`; other files, and folders the filter skips (`.vibranium/`, `.git/`), are
/// never reported, so a run's own reports and caches do not start another run. Reading a
/// file is not a save. A folder created or moved in is read when its batch goes out, so the
/// files saved into it before the OS watched it are reported too. Once the receiver is dropped,
/// the watch ends at the next batch.
#[derive(Clone, Debug)]
pub struct NotifyWatcher {
    root: PathBuf,
}

impl NotifyWatcher {
    /// A watcher for the project at `root`.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }
}

impl FileWatcher for NotifyWatcher {
    fn watch(&self, filter: &SourceFilter) -> Result<Receiver<Vec<SourcePath>>, WatchError> {
        let (events, raw) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(events).map_err(|error| WatchError::new(error.to_string()))?;
        watcher.watch(&self.root, RecursiveMode::Recursive).map_err(|error| WatchError::new(error.to_string()))?;
        let (send, batches) = mpsc::channel();
        let saved = Saved::new(&self.root, filter.clone());
        let thread = std::thread::Builder::new().name("vc-watch".to_owned());
        thread
            .spawn(move || {
                let _watching = watcher;
                debounce(&raw, &send, &saved);
            })
            .map_err(|error| WatchError::new(error.to_string()))?;
        Ok(batches)
    }
}

/// Which of an event's paths are saves to report: those under the root that the filter lets
/// through. The OS may report the root with its links resolved (`/private/var` on macOS), so
/// both spellings are stripped.
struct Saved {
    roots: Vec<PathBuf>,
    filter: SourceFilter,
}

impl Saved {
    fn new(root: &Path, filter: SourceFilter) -> Self {
        let mut roots = vec![root.to_path_buf()];
        roots.extend(std::fs::canonicalize(root).ok().filter(|resolved| resolved != root));
        Self { roots, filter }
    }

    /// The saved paths in `event`; none when the event only read a file.
    fn of(&self, event: &Event) -> Vec<SourcePath> {
        if event.kind.is_access() {
            return Vec::new();
        }
        event.paths.iter().filter_map(|path| self.path(path)).collect()
    }

    fn path(&self, path: &Path) -> Option<SourcePath> {
        let rest = self.roots.iter().find_map(|root| path.strip_prefix(root).ok())?;
        let text = rest.to_str()?.replace('\\', "/");
        if ROOT_FILES.contains(&text.as_str()) {
            return SourcePath::new(&text).ok();
        }
        self.filter.select(&text)?.ok()
    }

    /// The folders `event` created or moved in under the root that the filter does not skip.
    fn folders<'a>(&'a self, event: &'a Event) -> impl Iterator<Item = PathBuf> + 'a {
        let arrived = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_)));
        event.paths.iter().filter(move |path| arrived && path.is_dir() && !self.skips(path)).cloned()
    }

    fn skips(&self, folder: &Path) -> bool {
        let rest = self.roots.iter().find_map(|root| folder.strip_prefix(root).ok());
        rest.and_then(Path::to_str).is_none_or(|text| self.filter.skips(&text.replace('\\', "/")))
    }

    /// The saves to report among the files under `folder`.
    fn inside(&self, folder: &Path) -> Vec<SourcePath> {
        let walk = ignore::WalkBuilder::new(folder).standard_filters(false).follow_links(false).build();
        let files = walk.flatten().filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()));
        files.filter_map(|entry| self.path(entry.path())).collect()
    }
}

/// The saves of one burst: the paths reported, and the folders created or moved in. inotify
/// (Linux) starts watching a new folder only after reporting it, so a file saved into it at
/// once has no event of its own; the folders are read when the burst ends, after the OS
/// watches them.
#[derive(Default)]
struct Burst {
    paths: BTreeSet<SourcePath>,
    folders: BTreeSet<PathBuf>,
}

impl Burst {
    fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.folders.is_empty()
    }

    fn add(&mut self, saved: &Saved, event: &Event) {
        self.paths.extend(saved.of(event));
        self.folders.extend(saved.folders(event));
    }

    /// The burst's saves in path order, each once; the burst is empty after.
    fn take(&mut self, saved: &Saved) -> Vec<SourcePath> {
        let mut paths = std::mem::take(&mut self.paths);
        for folder in std::mem::take(&mut self.folders) {
            paths.extend(saved.inside(&folder));
        }
        paths.into_iter().collect()
    }
}

/// Collects saved paths until the files have been quiet for [`DEBOUNCE`], then sends them as
/// one batch, in path order and each once; ends when the events stop or the batches' receiver
/// is gone. Events with nothing to report never start a batch, and an empty batch is not sent.
fn debounce(events: &Events, batches: &Sender<Vec<SourcePath>>, saved: &Saved) {
    let mut burst = Burst::default();
    loop {
        match next(events, burst.is_empty()) {
            Ok(Ok(event)) => burst.add(saved, &event),
            Ok(Err(_)) => {}
            Err(RecvTimeoutError::Timeout) => {
                let batch = burst.take(saved);
                if !batch.is_empty() && batches.send(batch).is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The next event: waiting for as long as it takes when no batch is open, else for the rest
/// of the debounce window.
fn next(events: &Events, idle: bool) -> Result<notify::Result<Event>, RecvTimeoutError> {
    if idle { events.recv().map_err(|_| RecvTimeoutError::Disconnected) } else { events.recv_timeout(DEBOUNCE) }
}

#[cfg(test)]
#[path = "watcher_tests.rs"]
mod tests;
