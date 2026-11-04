//! System adapter: discovery and files, clock, random, environment, commands, git and config sources.
//!
//! So far it finds the project ([`find_project_root`]), reads its definition files
//! ([`FsSources`], a `SourceProvider`) and keeps the compile cache at
//! `.vibranium/cache/compile.bin` ([`FsCompileCache`], a `CompileCacheStore`) (D132, D133). It
//! gives the seeded random source ([`SeededRandom`], a `Random`), a snapshot of the process
//! environment ([`SystemEnvironment`], an `Environment`) and today's date in UTC
//! ([`SystemClock`], a `Clock`). The config layers above the project's files are
//! [`DotEnv`], [`VibVars`] and [`SetFlags`], each a `ConfigSource` (D129). [`ProcessRunner`]
//! runs a program with its arguments as a list, never through a shell (a `CommandRunner`);
//! [`SystemBrowser`] opens a sign-in URL with `open` or `xdg-open` through it (a `Browser`).
//! Nothing here reads or writes outside the project root: links that lead out of it are
//! reported, not followed.

mod browser;
mod clock;
mod command;
mod compile_cache;
mod config_sources;
mod environment;
mod git;
mod random;
mod root;
mod sources;

pub use browser::SystemBrowser;
pub use clock::SystemClock;
pub use command::ProcessRunner;
pub use compile_cache::FsCompileCache;
pub use config_sources::{DotEnv, SetFlags, VibVars};
pub use environment::SystemEnvironment;
pub use git::GitVcs;
pub use random::SeededRandom;
pub use root::{CONFIG_FILE, find_project_root};
pub use sources::FsSources;
