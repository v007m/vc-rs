//! Traits the core depends on: transport, clock, sources, reporters and the rest.
//!
//! Each trait is owned by the code that calls it and implemented by an adapter (and by a fake
//! in `vc-testkit`, which also holds the contract suite every implementation runs, D112). So far:
//!
//! - [`SourceProvider`]: finds and reads the project's definition files for the engine, which
//!   hands them to the pure compiler (D132).
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

mod auth;
mod browser;
mod cache;
mod clock;
mod command;
mod config;
mod environment;
mod filter;
mod history;
mod keychain;
mod random;
mod reporter;
mod secrets;
mod source;
mod transport;
mod vcs;

pub use auth::{ActionRunner, AuthError, AuthProvider, AuthRequest, Credentials};
pub use browser::{Browser, BrowserError};
pub use cache::{CompileCacheError, CompileCacheStore};
pub use clock::Clock;
pub use command::{CommandError, CommandOutput, CommandRunner, CommandSpec};
pub use config::{ConfigEntry, ConfigSource, ConfigSourceError};
pub use environment::Environment;
pub use filter::SourceFilter;
pub use history::{
    CheckRow, ExchangeRow, HistoryError, HistoryReader, HistoryWriter, Row, RunEnd, RunId, RunRef, RunRow,
    RunStart, TestRow, TimingRow,
};
pub use keychain::{Keychain, KeychainError};
pub use random::Random;
pub use reporter::{Artifact, ReportError, Reporter, RunSummary};
pub use secrets::{Secret, SecretError, SecretProvider, SecretQuery, Secrets};
pub use source::{Discovery, SourceError, SourceProvider, content_hash, source_file};
pub use transport::{SendContext, Transport, TransportError};
pub use vcs::{Vcs, VcsError, checked_ref};
