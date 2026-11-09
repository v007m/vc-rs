use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vc_adapter_plugins::FsPluginStore;
use vc_adapter_secrets::OsKeychain;
use vc_adapter_system::{FsCompileCache, FsSources, GitVcs, NotifyWatcher, ProcessRunner, SystemClock};
use vc_interfaces::{
    AgentActionWriter, Approvals, AuthProvider, Browser, CacheStore, Clock, CommandRunner, CompileCacheStore,
    Environment, FileWatcher, HistoryReader, HistoryWriter, HookRunner, Keychain, NotifierFactory, PluginStore,
    SourceProvider, SourceWriter, Transport, Vcs,
};

use crate::EngineError;

/// Puts an [`Engine`] together: the composition root, and the only place that names the
/// concrete adapters. Any port not given is filled with the system adapter for the root.
pub struct EngineBuilder {
    root: PathBuf,
    sources: Option<Arc<dyn SourceProvider>>,
    writer: Option<Arc<dyn SourceWriter>>,
    compile_cache: Option<Arc<dyn CompileCacheStore>>,
    clock: Option<Arc<dyn Clock>>,
    environment: Option<Arc<dyn Environment>>,
    transport: Option<Arc<dyn Transport>>,
    vcs: Option<Arc<dyn Vcs>>,
    commands: Option<Arc<dyn CommandRunner>>,
    hooks: Option<Arc<dyn HookRunner>>,
    user_config: Option<PathBuf>,
    history: Option<HistoryPorts>,
    approvals: Option<ApprovalPorts>,
    keychain: Option<Arc<dyn Keychain>>,
    cache: Option<Arc<dyn CacheStore>>,
    watcher: Option<Arc<dyn FileWatcher>>,
    auth: Option<Arc<dyn AuthProvider>>,
    browser: Option<Arc<dyn Browser>>,
    notifiers: Option<Arc<dyn NotifierFactory>>,
    plugins: Option<Arc<dyn PluginStore>>,
}

/// The history's writer and reader, given together.
type HistoryPorts = (Arc<dyn HistoryWriter>, Arc<dyn HistoryReader>);

/// The approval queue and the agent call log, given together.
type ApprovalPorts = (Arc<dyn Approvals>, Arc<dyn AgentActionWriter>);

impl EngineBuilder {
    /// A builder for the project whose root is `root`.
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self {
            root,
            sources: None,
            writer: None,
            compile_cache: None,
            clock: None,
            environment: None,
            transport: None,
            vcs: None,
            commands: None,
            hooks: None,
            user_config: None,
            history: None,
            approvals: None,
            keychain: None,
            cache: None,
            watcher: None,
            auth: None,
            browser: None,
            notifiers: None,
            plugins: None,
        }
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

    /// Writes the definition files authoring commands change with `writer` instead of into the
    /// project folder.
    #[must_use]
    pub fn writer(mut self, writer: Arc<dyn SourceWriter>) -> Self {
        self.writer = Some(writer);
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

    /// Reads `${env.X}`, the `VIB_*` settings and the user config's folder from `environment`
    /// instead of the process environment; the project's `.env` file still fills what it does
    /// not set.
    #[must_use]
    pub fn environment(mut self, environment: Arc<dyn Environment>) -> Self {
        self.environment = Some(environment);
        self
    }

    /// Sends requests through `transport` instead of HTTP.
    #[must_use]
    pub fn transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.transport = Some(transport);
        self
    }

    /// Asks `vcs` which files changed (`--changed`) instead of git.
    #[must_use]
    pub fn vcs(mut self, vcs: Arc<dyn Vcs>) -> Self {
        self.vcs = Some(vcs);
        self
    }

    /// Runs `command:` actions with `commands` instead of the system shell.
    #[must_use]
    pub fn commands(mut self, commands: Arc<dyn CommandRunner>) -> Self {
        self.commands = Some(commands);
        self
    }

    /// Runs hooks with `hooks` instead of `QuickJS`.
    #[must_use]
    pub fn hooks(mut self, hooks: Arc<dyn HookRunner>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Reads the user config from `path` instead of `~/.config/vibranium/config.yaml`.
    #[must_use]
    pub fn user_config(mut self, path: PathBuf) -> Self {
        self.user_config = Some(path);
        self
    }

    /// Keeps the run history in `writer` and reads it from `reader` instead of
    /// `.vibranium/history.db`.
    #[must_use]
    pub fn history(mut self, writer: Arc<dyn HistoryWriter>, reader: Arc<dyn HistoryReader>) -> Self {
        self.history = Some((writer, reader));
        self
    }

    /// Keeps the agent approval queue in `queue` and logs agent calls to `log` instead of
    /// `.vibranium/history.db` (D195).
    #[must_use]
    pub fn approvals(mut self, queue: Arc<dyn Approvals>, log: Arc<dyn AgentActionWriter>) -> Self {
        self.approvals = Some((queue, log));
        self
    }

    /// Keeps the cross-run cache's key in `keychain` instead of the OS keychain.
    #[must_use]
    pub fn keychain(mut self, keychain: Arc<dyn Keychain>) -> Self {
        self.keychain = Some(keychain);
        self
    }

    /// Keeps the cross-run cache in `store` instead of `.vibranium/cache.db`, with no key.
    #[must_use]
    pub fn cache(mut self, store: Arc<dyn CacheStore>) -> Self {
        self.cache = Some(store);
        self
    }

    /// Learns which files were saved from `watcher` instead of the OS's file events
    /// (`vc watch`).
    #[must_use]
    pub fn watcher(mut self, watcher: Arc<dyn FileWatcher>) -> Self {
        self.watcher = Some(watcher);
        self
    }

    /// Obtains credentials through `auth` instead of the standard auth types (D106).
    #[must_use]
    pub fn auth(mut self, auth: Arc<dyn AuthProvider>) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Opens the `oauth2AuthorizationCode` sign-in page with `browser` instead of the system
    /// browser.
    #[must_use]
    pub fn browser(mut self, browser: Arc<dyn Browser>) -> Self {
        self.browser = Some(browser);
        self
    }

    /// Sends the `alerts` notices through `notifiers` instead of the standard channels.
    #[must_use]
    pub fn notifiers(mut self, notifiers: Arc<dyn NotifierFactory>) -> Self {
        self.notifiers = Some(notifiers);
        self
    }

    /// Keeps plugins in `store` instead of `.vibranium/plugins/` and `vibranium.lock`.
    #[must_use]
    pub fn plugins(mut self, store: Arc<dyn PluginStore>) -> Self {
        self.plugins = Some(store);
        self
    }

    /// The engine.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project folder cannot be opened.
    pub fn build(self) -> Result<Engine, EngineError> {
        let folder = FsSources::new(&self.root);
        let sources: Arc<dyn SourceProvider> = match self.sources {
            Some(sources) => sources,
            None => Arc::new(folder.clone().map_err(|error| EngineError::Sources { reason: error.to_string() })?),
        };
        let writer = self.writer.or_else(|| folder.ok().map(|folder| Arc::new(folder) as Arc<dyn SourceWriter>));
        let compile_cache = self.compile_cache.unwrap_or_else(|| Arc::new(FsCompileCache::new(&self.root)));
        let clock = self.clock.unwrap_or_else(|| Arc::new(SystemClock));
        let vcs = self.vcs.unwrap_or_else(|| Arc::new(GitVcs::new(&self.root)));
        let commands = self.commands.unwrap_or_else(|| Arc::new(ProcessRunner::new()));
        let (history, history_reader) = self.history.unzip();
        let keychain =
            self.keychain.or_else(|| OsKeychain::platform().ok().map(|os| Arc::new(os) as Arc<dyn Keychain>));
        let (cache, watcher, auth, browser) = (self.cache, self.watcher, self.auth, self.browser);
        let watcher = watcher.unwrap_or_else(|| Arc::new(NotifyWatcher::new(&self.root)));
        Ok(Engine {
            plugins: self.plugins.unwrap_or_else(|| Arc::new(FsPluginStore::new(&self.root))),
            root: self.root,
            sources,
            writer,
            compile_cache,
            clock,
            environment: self.environment,
            transport: self.transport,
            vcs,
            commands,
            hooks: self.hooks,
            user_config: self.user_config,
            history,
            history_reader,
            approvals: self.approvals,
            keychain,
            cache,
            watcher,
            auth,
            browser,
            notifiers: self.notifiers,
        })
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
    /// `None` only when the sources were given and the root is not a folder to write into.
    pub(crate) writer: Option<Arc<dyn SourceWriter>>,
    pub(crate) compile_cache: Arc<dyn CompileCacheStore>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) environment: Option<Arc<dyn Environment>>,
    pub(crate) transport: Option<Arc<dyn Transport>>,
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
