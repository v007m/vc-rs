//! The engine apps talk to, and its ports as the builder filled them.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_plugins::Confiner;
use vc_interfaces::{
    AuthProvider, Browser, CacheStore, Clock, CommandRunner, CompileCacheStore, Environment, FileWatcher,
    HistoryReader, HistoryWriter, HookRunner, Keychain, NotifierFactory, PluginStore, SourceProvider, SourceWriter,
    TransportFactory, Vcs,
};

use crate::builder::{ApprovalPorts, PluginProcesses};
/// The engine apps talk to: one per project.
pub struct Engine {
    pub(crate) root: PathBuf,
    pub(crate) sources: Arc<dyn SourceProvider>,
    /// `None` only when the sources were given and the root is not a folder to write into.
    pub(crate) writer: Option<Arc<dyn SourceWriter>>,
    pub(crate) compile_cache: Arc<dyn CompileCacheStore>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) environment: Option<Arc<dyn Environment>>,
    pub(crate) transports: Arc<dyn TransportFactory>,
    pub(crate) vcs: Arc<dyn Vcs>,
    pub(crate) commands: Arc<dyn CommandRunner>,
    pub(crate) hooks: Option<Arc<dyn HookRunner>>,
    pub(crate) user_config: Option<PathBuf>,
    pub(crate) history: Option<Arc<dyn HistoryWriter>>,
    pub(crate) history_reader: Option<Arc<dyn HistoryReader>>,
    pub(crate) approvals: Option<ApprovalPorts>,
    pub(crate) keychain: Option<Arc<dyn Keychain>>,
    pub(crate) cache: Option<Arc<dyn CacheStore>>,
    pub(crate) watcher: Arc<dyn FileWatcher>,
    pub(crate) auth: Option<Arc<dyn AuthProvider>>,
    pub(crate) browser: Option<Arc<dyn Browser>>,
    pub(crate) notifiers: Option<Arc<dyn NotifierFactory>>,
    pub(crate) plugins: Arc<dyn PluginStore>,
    pub(crate) plugin_processes: PluginProcesses,
    /// How every plugin host confines its plugin, asking the OS once whether it can.
    pub(crate) confiner: Arc<Confiner>,
}

impl Engine {
    /// The project root, as the builder was given it.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `path`, from `base` when it is relative, as a path in the project with `/` between its
    /// parts, links resolved; `None` when it is not a file or folder in the project.
    #[must_use]
    pub fn project_path(&self, base: &Path, path: &Path) -> Option<String> {
        vc_adapter_system::project_path(&self.root, &base.join(path))
    }
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine").field("root", &self.root).finish_non_exhaustive()
    }
}
