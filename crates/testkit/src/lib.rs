//! Test fakes, fixture builders and one contract suite per interface trait (dev-dependency only).
//!
//! Every implementation of a `vc-interfaces` trait, fakes included, runs that trait's suite from
//! a test (`<snake_trait>_suite`, D112); `arch-tests` checks that it does:
//!
//! ```
//! use vc_interfaces::SourceProvider;
//! use vc_testkit::{MemorySources, source_provider_suite};
//!
//! source_provider_suite(&mut |files| Box::new(MemorySources::new(files)) as Box<dyn SourceProvider>);
//! ```

mod cache_suite;
mod memory_cache;
mod memory_sources;
mod source_suite;

pub use cache_suite::{MakeCompileCache, compile_cache_store_suite};
pub use memory_cache::MemoryCompileCache;
pub use memory_sources::MemorySources;
pub use source_suite::{MakeSources, source_provider_suite};
