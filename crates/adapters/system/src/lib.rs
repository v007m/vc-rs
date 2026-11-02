//! System adapter: discovery and files, clock, random, environment, commands, git and config sources.
//!
//! So far it finds the project ([`find_project_root`]), reads its definition files
//! ([`FsSources`], a `SourceProvider`) and keeps the compile cache at
//! `.vibranium/cache/compile.bin` ([`FsCompileCache`], a `CompileCacheStore`) (D132, D133).
//! Nothing here reads or writes outside the project root: links that lead out of it are
//! reported, not followed.

mod compile_cache;
mod root;
mod sources;

pub use compile_cache::FsCompileCache;
pub use root::{CONFIG_FILE, find_project_root};
pub use sources::FsSources;
