//! Traits the core depends on: transport, clock, sources, reporters and the rest.
//!
//! Each trait is owned by the code that calls it and implemented by an adapter (and by a fake
//! in `vc-testkit`, which also holds the contract suite every implementation runs, D112). So far:
//!
//! - [`SourceProvider`]: finds and reads the project's files for the engine, which
//!   hands them to the pure compiler (D132).
//! - [`SourceWriter`]: writes the definition files authoring commands change, after the
//!   engine's compile guard passed them (D107).
//! - [`CompileCacheStore`]: keeps the compiler's cache between runs as one opaque blob.
//! - [`Transport`]: sends one rendered request and returns the [`vc_model::Exchange`] with its
//!   timings; auth, retry, timeout and per-host limits are executor layers around it (D141).
//! - [`Random`]: the seeded random source for the `random` and `shuffle` pipes and fake data.
//! - [`Environment`]: the process environment for `${env.NAME}`.
//! - [`Clock`]: today's date in UTC, for the lints that compare dates (an expired `skip`), and
//!   the time a run started.
//! - [`ConfigSource`]: one config layer around the project's files: the user config, `.env`,
//!   `VIB_*`, `--set` (D129).
//! - [`Reporter`]: turns the run's events into console output or a report file (D43).
//! - [`SecretProvider`]: resolves the `${secret.*}` names `.env.schema` declares, once per run
//!   (D163, D165).
//! - [`CommandRunner`]: runs a program with its arguments as a list, never through a shell.
//! - [`AuthProvider`]: obtains [`Credentials`] for one `auth` type; [`ActionRunner`] runs a
//!   `flow` definition's steps for it (D106).
//! - [`Browser`] and [`Keychain`]: the browser sign-in and the stored refresh token of
//!   `oauth2AuthorizationCode`.
//! - [`HistoryWriter`] and [`HistoryReader`]: the run history in `.vibranium/history.db`; the
//!   writer stores redacted rows and the report snapshot, the reader answers `vc history`,
//!   `vc report` and `vc run --failed` (D167, D170).
//! - [`CacheStore`]: the cross-run cache in `.vibranium/cache.db`, what a `cache:`d action
//!   saved and the cleanups that undo it, encrypted at rest by the store.
//! - [`FileWatcher`]: the batches of saved project files `vc watch` re-runs on (D180).
//! - [`HookRunner`]: runs a hook function in a sandbox within a time and memory [`Budget`] (D172).
//! - [`Importer`]: reads a foreign file (an `OpenAPI` spec, a HAR file) into an [`ImportDraft`];
//!   [`SpecFetcher`] fetches the documents its `$ref`s name.
//! - [`Approvals`] and [`AgentActionLog`]: the agent approval queue and the agent call log in
//!   `history.db` (D195).
//! - [`Notifier`] and [`NotifierFactory`]: send an alert rule's [`Notice`] to a webhook, Slack, Teams
//!   or `PagerDuty` channel at the end of a run; a failure is a V5117 warning.
//! - [`PluginHost`]: one running plugin, called over JSON-RPC within a deadline, given only the
//!   secrets its [`PluginManifest`] declares.
//! - [`PluginStore`]: fetches a plugin for `vc plugin add`, installs it under
//!   `.vibranium/plugins/`, and reads `vibranium.lock` and the installed manifests.
//!
//! ```
//! use vc_interfaces::{SourceFilter, content_hash, source_file};
//! use vc_model::SourcePath;
//!
//! let filter = SourceFilter::default();
//! let path = SourcePath::new("node_modules/x/x.api.yaml").unwrap();
//! assert!(!filter.allows(&path));
//! let file = source_file(SourcePath::new("users.api.yaml").unwrap(), b"apis: []\n".to_vec()).unwrap();
//! assert_eq!(file.hash, content_hash(b"apis: []\n"));
//! ```

mod action_cache;
mod agent_log;
mod approvals;
mod auth;
mod browser;
mod cache;
mod clock;
mod command;
mod config;
mod environment;
mod fetch;
mod filter;
mod history;
mod hooks;
mod import;
mod keychain;
mod notifier;
mod plugin;
mod plugin_store;
mod random;
mod reporter;
mod secrets;
mod source;
mod transport;
mod trend;
mod vcs;
mod watch;

pub use action_cache::{CacheEntry, CacheKey, CacheListing, CacheStore, CacheStoreError, CachedCleanup, CachedValues};
pub use agent_log::{AgentActionLog, AgentActionReader, LoggedAction};
pub use approvals::{
    AgentAction, AgentActionWriter, Approval, ApprovalError, ApprovalId, ApprovalRequest, ApprovalStatus, Approvals,
    Decided, Decision, StandingRule, Verdict,
};
pub use auth::{ActionRunner, AuthError, AuthProvider, AuthRequest, Credentials};
pub use browser::{Browser, BrowserError};
pub use cache::{CompileCacheError, CompileCacheStore};
pub use clock::Clock;
pub use command::{CommandError, CommandOutput, CommandRunner, CommandSpec};
pub use config::{ConfigEntry, ConfigSource, ConfigSourceError};
pub use environment::Environment;
pub use fetch::{FetchError, SpecFetcher, resolve_reference};
pub use filter::SourceFilter;
pub use history::{
    CheckRow, ExchangeRow, HistoryError, HistoryReader, HistoryWriter, IntervalRow, Row, RunEnd, RunId, RunRef, RunRow,
    RunStart, TestRow, TimingRow,
};
pub use hooks::{Budget, HookCall, HookError, HookOutput, HookRunner, Limit};
pub use import::{
    DraftApi, DraftBefore, DraftCleanup, DraftExample, DraftFlow, DraftGroup, DraftStep, DraftTest, ImportDraft,
    ImportError, ImportNote, ImportOptions, ImportSource, Importer, RecordingFilter, SecretFinding,
};
pub use keychain::{Keychain, KeychainError};
pub use notifier::{Notice, NoticeFailure, Notifier, NotifierFactory, NotifyError};
pub use plugin::{
    CALL_DEADLINE, DEADLINE_PASSED, INITIALIZE_DEADLINE, PERMISSION_DENIED, PluginError, PluginHost, PluginKind,
    PluginManifest, PluginPermissions, PluginStderr,
};
pub use plugin_store::{Fetched, LOCK_FILE, MANIFEST_FILE, PLUGINS_DIR, PluginStore, PluginStoreError};
pub use random::Random;
pub use reporter::{Artifact, ReportError, Reporter, RunSummary};
pub use secrets::{Secret, SecretError, SecretProvider, SecretQuery, Secrets};
pub use source::{Discovery, SourceError, SourceProvider, SourceWriter, content_hash, source_file};
pub use transport::{SendContext, Transport, TransportError};
pub use trend::TestTrend;
pub use vcs::{Vcs, VcsError, checked_ref};
pub use watch::{FileWatcher, WatchError};
