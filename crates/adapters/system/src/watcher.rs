use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use notify::{Event, RecursiveMode, Watcher as _};
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
/// A batch holds the definition files the filter lets through, and the root `.env` and
/// `.env.schema`; other files, and folders the filter skips (`.vibranium/`, `.git/`), are
/// never reported, so a run's own reports and caches do not start another run. Reading a
/// file is not a save. Once the receiver is dropped, the watch ends at the next file event.
#[derive(Clone, Debug)]
pub struct NotifyWatcher {
    root: PathBuf,
    filter: SourceFilter,
}

impl NotifyWatcher {
    /// A watcher for the project at `root`, reporting what `filter` lets through.
    #[must_use]
    pub fn new(root: &Path, filter: SourceFilter) -> Self {
        Self { root: root.to_path_buf(), filter }
    }
}

impl FileWatcher for NotifyWatcher {
    fn watch(&self) -> Result<Receiver<Vec<SourcePath>>, WatchError> {
        let (events, raw) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(events).map_err(|error| WatchError::new(error.to_string()))?;
        watcher.watch(&self.root, RecursiveMode::Recursive).map_err(|error| WatchError::new(error.to_string()))?;
        let (send, batches) = mpsc::channel();
        let saved = Saved::new(&self.root, self.filter.clone());
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
}

/// Collects saved paths until the files have been quiet for [`DEBOUNCE`], then sends them as
/// one batch, in path order and each once; ends when the events stop or the batches' receiver
/// is gone. Events with nothing to report never start a batch.
fn debounce(events: &Events, batches: &Sender<Vec<SourcePath>>, saved: &Saved) {
    let mut batch = BTreeSet::new();
    loop {
        match next(events, batch.is_empty()) {
            Ok(Ok(event)) => batch.extend(saved.of(&event)),
            Ok(Err(_)) => {}
            Err(RecvTimeoutError::Timeout) => {
                if batches.send(std::mem::take(&mut batch).into_iter().collect()).is_err() {
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
