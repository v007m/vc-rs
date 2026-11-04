//! System adapter: discovery and files, clock, random, environment, commands, git and config sources.
//!
//! So far it finds the project ([`find_project_root`]), reads its definition files
//! ([`FsSources`], a `SourceProvider`) and keeps the compile cache at
//! `.vibranium/cache/compile.bin` ([`FsCompileCache`], a `CompileCacheStore`) (D132, D133). It
//! gives the seeded random source ([`SeededRandom`], a `Random`), a snapshot of the process
//! environment ([`SystemEnvironment`], an `Environment`) and today's date in UTC
//! ([`SystemClock`], a `Clock`).
//! Nothing here reads or writes outside the project root: links that lead out of it are
//! reported, not followed.

mod clock;
mod compile_cache;
mod environment;
mod git;
mod random;
mod root;
mod sources;

pub use clock::SystemClock;
pub use compile_cache::FsCompileCache;
pub use environment::SystemEnvironment;
pub use git::GitVcs;
pub use random::SeededRandom;
pub use root::{CONFIG_FILE, find_project_root};
pub use sources::FsSources;
