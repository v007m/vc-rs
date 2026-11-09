//! Watching the project's files, for `vc watch` (D180).

use std::sync::mpsc::Receiver;

use vc_model::SourcePath;

/// Tells which of the project's files were saved, so `vc watch` re-runs what they affect.
///
/// The adapter watches the project root, drops the paths the [`crate::SourceFilter`] refuses,
/// and debounces: the saves of one burst (an editor writing several files, a formatter run)
/// arrive as one batch once the files have been quiet for the debounce window (200 ms).
pub trait FileWatcher: Send + Sync {
    /// Starts watching; each batch of saved paths, relative to the root and in path order,
    /// arrives on the returned channel. Dropping the receiver stops the watch.
    ///
    /// # Errors
    ///
    /// [`WatchError`] when the OS refuses to watch the root.
    fn watch(&self) -> Result<Receiver<Vec<SourcePath>>, WatchError>;
}

/// Why the files cannot be watched.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot watch the project: {reason}")]
#[non_exhaustive]
pub struct WatchError {
    /// What went wrong.
    pub reason: String,
}

impl WatchError {
    /// An error for `reason`.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::WatchError;

    #[test]
    fn says_why_it_cannot_watch() {
        assert_eq!(WatchError::new("too many open files").to_string(), "cannot watch the project: too many open files");
    }
}
