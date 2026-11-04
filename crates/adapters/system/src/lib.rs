//! System adapter: discovery and files, clock, random, environment, commands, git and config sources.
//!
//! So far it finds the project ([`find_project_root`]), reads its definition files
//! ([`FsSources`], a `SourceProvider`) and keeps the compile cache at
//! `.vibranium/cache/compile.bin` ([`FsCompileCache`], a `CompileCacheStore`) (D132, D133). It
//! gives the seeded random source ([`SeededRandom`], a `Random`), a snapshot of the process
//! environment ([`SystemEnvironment`], an `Environment`) and today's date in UTC
//! ([`SystemClock`], a `Clock`). The config layers above the project's files are
//! [`DotEnv`], [`VibVars`] and [`SetFlags`], each a `ConfigSource` (D129).
//! Nothing here reads or writes outside the project root: links that lead out of it are
//! reported, not followed.

mod clock;
mod compile_cache;
mod config_sources;
mod environment;
mod random;
mod root;
mod sources;

pub use clock::SystemClock;
pub use compile_cache::FsCompileCache;
pub use config_sources::{DotEnv, SetFlags, VibVars};
pub use environment::SystemEnvironment;
pub use random::SeededRandom;
pub use root::{CONFIG_FILE, find_project_root};
pub use sources::FsSources;
