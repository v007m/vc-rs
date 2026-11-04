//! System adapter: discovery and files, clock, random, environment, commands, git and config sources.
//!
//! So far it finds the project ([`find_project_root`]), reads its definition files
//! ([`FsSources`], a `SourceProvider`) and keeps the compile cache at
//! `.vibranium/cache/compile.bin` ([`FsCompileCache`], a `CompileCacheStore`) (D132, D133). It
//! gives the seeded random source ([`SeededRandom`], a `Random`) and a snapshot of the process
//! environment ([`SystemEnvironment`], an `Environment`).
//! Nothing here reads or writes outside the project root: links that lead out of it are
//! reported, not followed.

mod compile_cache;
mod environment;
mod random;
mod root;
mod sources;

pub use compile_cache::FsCompileCache;
pub use environment::SystemEnvironment;
pub use random::SeededRandom;
pub use root::{CONFIG_FILE, find_project_root};
pub use sources::FsSources;
