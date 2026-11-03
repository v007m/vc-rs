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

mod cache;
mod filter;
mod source;
mod transport;

pub use cache::{CompileCacheError, CompileCacheStore};
pub use filter::SourceFilter;
pub use source::{Discovery, SourceError, SourceProvider, content_hash, source_file};
pub use transport::{SendContext, Transport, TransportError};
