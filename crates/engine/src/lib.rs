//! Engine facade and composition root: the only crate apps import.
//!
//! [`EngineBuilder`] is the one place that names the concrete adapters; apps get an
//! [`Engine`] and call it. So far the engine checks a project ([`Engine::check`]) and exports
//! the editor schemas ([`Engine::schema_export`]). The model and diagnostics types apps need
//! are re-exported here (D108).
//!
//! ```
//! use vc_engine::Engine;
//!
//! let schemas = Engine::schema_export().unwrap();
//! assert_eq!(schemas[0].file_name, "config.schema.json");
//! ```

mod builder;
mod check;
mod error;
mod problems;
mod schema;
mod selection;

pub use builder::{Engine, EngineBuilder};
pub use check::{CheckReport, CheckRequest};
pub use error::EngineError;
pub use schema::SchemaFile;
pub use vc_diagnostics::{Diagnostics, DiagnosticsError, JsonRenderer, Render, Summary, TextRenderer};
pub use vc_model::{FileKind, SourceFiles};
