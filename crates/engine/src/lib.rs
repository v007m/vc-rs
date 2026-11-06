//! Engine facade and composition root: the only crate apps import.
//!
//! [`EngineBuilder`] is the one place that names the concrete adapters; apps get an
//! [`Engine`] and call it. So far the engine checks a project ([`Engine::check`]), plans and
//! runs its tests ([`Engine::plan`], [`Engine::run`]), lists its definitions
//! ([`Engine::list`]), lists and clears the cross-run cache ([`Engine::cache_list`],
//! [`Engine::cache_clear`]) and exports the editor schemas ([`Engine::schema_export`]). The model and
//! diagnostics types apps need are re-exported here (D108).
//!
//! ```
//! use vc_engine::Engine;
//!
//! let schemas = Engine::schema_export().unwrap();
//! assert_eq!(schemas[0].file_name, "config.schema.json");
//! ```

mod builder;
mod cache;
mod check;
mod error;
mod failed;
mod history;
mod interrupt;
mod list;
mod plan;
mod ports;
mod problems;
mod project;
mod reporting;
mod request;
mod run;
#[cfg(test)]
mod sample;
mod schema;
mod selection;
mod settings;
mod stored;
mod user;

pub use builder::{Engine, EngineBuilder};
pub use cache::CacheReport;
pub use check::{CheckReport, CheckRequest};
pub use error::EngineError;
pub use interrupt::{Interrupter, Interrupts, interrupts};
pub use list::{ListReport, Listed};
pub use plan::PlanReport;
pub use request::{ListKind, ListRequest, PlanRequest, ReportFlags, RunRequest, SelectRequest};
pub use run::RunReport;
pub use schema::SchemaFile;
pub use stored::{HistoryReport, ReportFiles, ReportRequest, StoredRun};
pub use vc_diagnostics::{Diagnostics, DiagnosticsError, JsonRenderer, Render, Summary, TextRenderer};
pub use vc_model::{FileKind, SourceFiles};
