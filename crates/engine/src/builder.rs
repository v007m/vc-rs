use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_system::{FsCompileCache, FsSources, SystemClock};
use vc_interfaces::{Clock, CompileCacheStore, SourceProvider};

use crate::EngineError;

/// Puts an [`Engine`] together: the composition root, and the only place that names the
/// concrete adapters. Any port not given is filled with the system adapter for the root.
pub struct EngineBuilder {
    root: PathBuf,
    sources: Option<Arc<dyn SourceProvider>>,
    compile_cache: Option<Arc<dyn CompileCacheStore>>,
    clock: Option<Arc<dyn Clock>>,
}

impl EngineBuilder {
    /// A builder for the project whose root is `root`.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root, sources: None, compile_cache: None, clock: None }
    }

    /// A builder for the nearest project at or above `start`: the first folder holding a
    /// `vibranium.config.yaml`.
    ///
    /// # Errors
    ///
    /// [`EngineError::NoProject`] when there is none.
    pub fn for_project(start: &Path) -> Result<Self, EngineError> {
        vc_adapter_system::find_project_root(start)
            .map(Self::new)
            .ok_or_else(|| EngineError::NoProject { start: start.display().to_string() })
    }

    /// Reads definition files from `sources` instead of the project folder.
    #[must_use]
    pub fn sources(mut self, sources: Arc<dyn SourceProvider>) -> Self {
        self.sources = Some(sources);
        self
    }

    /// Keeps the compile cache in `store` instead of `.vibranium/cache/compile.bin`.
    #[must_use]
    pub fn compile_cache(mut self, store: Arc<dyn CompileCacheStore>) -> Self {
        self.compile_cache = Some(store);
        self
    }

    /// Takes today's date from `clock` instead of the system clock.
    #[must_use]
    pub fn clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = Some(clock);
        self
    }

    /// The engine.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project folder cannot be opened.
    pub fn build(self) -> Result<Engine, EngineError> {
        let sources: Arc<dyn SourceProvider> = match self.sources {
            Some(sources) => sources,
            None => Arc::new(
                FsSources::new(&self.root).map_err(|error| EngineError::Sources { reason: error.to_string() })?,
            ),
        };
        let compile_cache = self.compile_cache.unwrap_or_else(|| Arc::new(FsCompileCache::new(&self.root)));
        let clock = self.clock.unwrap_or_else(|| Arc::new(SystemClock));
        Ok(Engine { root: self.root, sources, compile_cache, clock })
    }
}

impl fmt::Debug for EngineBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineBuilder").field("root", &self.root).finish_non_exhaustive()
    }
}

/// The engine apps talk to: one per project.
pub struct Engine {
    root: PathBuf,
    pub(crate) sources: Arc<dyn SourceProvider>,
    pub(crate) compile_cache: Arc<dyn CompileCacheStore>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl Engine {
    /// The project root, as the builder was given it.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine").field("root", &self.root).finish_non_exhaustive()
    }
}
