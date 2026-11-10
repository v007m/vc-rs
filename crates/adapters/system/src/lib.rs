//! System adapter: discovery and files, clock, random, environment, commands, git and config sources.
//!
//! So far it finds the project ([`find_project_root`]), reads its definition files
//! ([`FsSources`], a `SourceProvider`) and keeps the compile cache at
//! `.vibranium/cache/compile.bin` ([`FsCompileCache`], a `CompileCacheStore`) (D132, D133). It
//! gives the seeded random source ([`SeededRandom`], a `Random`), a snapshot of the process
//! environment ([`SystemEnvironment`], an `Environment`) and today's date in UTC
//! ([`SystemClock`], a `Clock`). The config layers around the project's files are
//! [`UserConfig`] below them and [`DotEnv`], [`VibVars`] and [`SetFlags`] above them, each a
//! `ConfigSource` (D129, D182). [`ProcessRunner`] runs a program with its arguments as a list,
//! never through a shell of its own (a `CommandRunner`), keeping the tail of each output stream
//! and ending the whole process group on a timeout (D181); [`SystemBrowser`] opens a sign-in
//! URL with `open` or `xdg-open` through it (a `Browser`). [`NotifyWatcher`] reports the
//! project files saved, in batches 200 ms apart, for `vc watch` (a `FileWatcher`, D180, D190).
//! Nothing here reads or writes outside the project root but the user config: links that lead
//! out of it are reported, not followed.

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
mod user_config;
mod watcher;

pub use browser::SystemBrowser;
pub use clock::SystemClock;
pub use command::ProcessRunner;
pub use compile_cache::FsCompileCache;
pub use config_sources::{DotEnv, SetFlags, VibVars};
pub use environment::SystemEnvironment;
pub use git::GitVcs;
pub use random::SeededRandom;
pub use root::{CONFIG_FILE, find_project_root, project_path};
pub use sources::FsSources;
pub use user_config::UserConfig;
pub use watcher::NotifyWatcher;
