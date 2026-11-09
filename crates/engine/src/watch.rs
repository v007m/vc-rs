//! `vc watch`: the batches of saved files that re-run the selection (D180).

use std::sync::mpsc::Receiver;

use vc_model::SourcePath;

use crate::{Engine, EngineError};

impl Engine {
    /// Starts watching the project's files. Each batch of saved definition files that the
    /// config's `paths` let through, as discovery does (and the root `.env` and `.env.schema`),
    /// arrives on the receiver 200 ms after the last save of a burst, ready for
    /// [`crate::SelectRequest::saved`]. Dropping the receiver ends the watch.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project cannot be read, a `paths` glob does not
    /// parse, or the OS refuses to watch the project folder.
    pub fn watch(&self) -> Result<Receiver<Vec<SourcePath>>, EngineError> {
        let filter = self.source_filter()?;
        self.watcher.watch(&filter).map_err(|error| EngineError::Sources { reason: error.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, mpsc};

    use vc_interfaces::{FileWatcher, SourceFilter, WatchError};
    use vc_model::SourcePath;
    use vc_testkit::{FakeVcs, MemoryCompileCache, MemorySources};

    use crate::EngineBuilder;
    use crate::sample::builder;

    /// Reports one batch of the paths the filter lets through, or refuses to watch when it has
    /// none.
    struct OneBatch(Option<Vec<&'static str>>);

    impl FileWatcher for OneBatch {
        fn watch(&self, filter: &SourceFilter) -> Result<mpsc::Receiver<Vec<SourcePath>>, WatchError> {
            let saved = self.0.clone().ok_or_else(|| WatchError::new("too many open files"))?;
            let batch = saved.into_iter().filter_map(|path| filter.select(path)?.ok()).collect();
            let (send, batches) = mpsc::channel();
            send.send(batch).map_err(|error| WatchError::new(error.to_string()))?;
            Ok(batches)
        }
    }

    fn paths(batch: &[SourcePath]) -> Vec<&str> {
        batch.iter().map(SourcePath::as_str).collect()
    }

    #[test]
    fn hands_on_the_watchers_batches() {
        let batch = OneBatch(Some(vec!["services/users/crud.test.yaml"]));
        let engine = builder(FakeVcs::default()).watcher(Arc::new(batch)).build().unwrap();
        assert_eq!(paths(&engine.watch().unwrap().recv().unwrap()), ["services/users/crud.test.yaml"]);
    }

    #[test]
    fn watches_through_the_config_s_paths_as_discovery_does() {
        let config =
            "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\npaths:\n  exclude: [services/legacy]\n";
        let files: [(&str, &[u8]); 1] = [("vibranium.config.yaml", config.as_bytes())];
        let saved = OneBatch(Some(vec!["services/legacy/old.test.yaml", "services/users/crud.test.yaml"]));
        let engine = EngineBuilder::new(PathBuf::from("/project"))
            .sources(Arc::new(MemorySources::new(&files)))
            .compile_cache(Arc::new(MemoryCompileCache::new()))
            .watcher(Arc::new(saved))
            .build()
            .unwrap();
        assert_eq!(paths(&engine.watch().unwrap().recv().unwrap()), ["services/users/crud.test.yaml"]);
    }

    #[test]
    fn a_folder_the_os_will_not_watch_is_a_sources_error() {
        let engine = builder(FakeVcs::default()).watcher(Arc::new(OneBatch(None))).build().unwrap();
        let error = engine.watch().unwrap_err();
        assert_eq!(error.to_string(), "cannot watch the project: too many open files");
    }
}
