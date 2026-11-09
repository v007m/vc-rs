//! `vc watch`: the batches of saved files that re-run the selection (D180).

use std::sync::mpsc::Receiver;

use vc_model::SourcePath;

use crate::{Engine, EngineError};

impl Engine {
    /// Starts watching the project's files. Each batch of saved definition files (and the
    /// root `.env` and `.env.schema`) arrives on the receiver 200 ms after the last save of a
    /// burst, ready for [`crate::SelectRequest::saved`]. Dropping the receiver ends the watch.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the OS refuses to watch the project folder.
    pub fn watch(&self) -> Result<Receiver<Vec<SourcePath>>, EngineError> {
        self.watcher.watch().map_err(|error| EngineError::Sources { reason: error.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, mpsc};

    use vc_interfaces::{FileWatcher, WatchError};
    use vc_model::SourcePath;
    use vc_testkit::FakeVcs;

    use crate::sample::builder;

    /// Reports one batch, or refuses to watch when it has none.
    struct OneBatch(Option<Vec<SourcePath>>);

    impl FileWatcher for OneBatch {
        fn watch(&self) -> Result<mpsc::Receiver<Vec<SourcePath>>, WatchError> {
            let batch = self.0.clone().ok_or_else(|| WatchError::new("too many open files"))?;
            let (send, batches) = mpsc::channel();
            send.send(batch).map_err(|error| WatchError::new(error.to_string()))?;
            Ok(batches)
        }
    }

    #[test]
    fn hands_on_the_watchers_batches() {
        let batch = vec![SourcePath::new("services/users/crud.test.yaml").unwrap()];
        let engine = builder(FakeVcs::default()).watcher(Arc::new(OneBatch(Some(batch.clone())))).build().unwrap();
        assert_eq!(engine.watch().unwrap().recv().unwrap(), batch);
    }

    #[test]
    fn a_folder_the_os_will_not_watch_is_a_sources_error() {
        let engine = builder(FakeVcs::default()).watcher(Arc::new(OneBatch(None))).build().unwrap();
        let error = engine.watch().unwrap_err();
        assert_eq!(error.to_string(), "cannot watch the project: too many open files");
    }
}
