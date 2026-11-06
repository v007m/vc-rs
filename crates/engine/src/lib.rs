//! Engine facade and composition root: the only crate apps import.
//!
//! [`EngineBuilder`] is the one place that names the concrete adapters; apps get an
//! [`Engine`] and call it. So far the engine checks a project ([`Engine::check`]), plans and
//! runs its tests ([`Engine::plan`], [`Engine::run`]), lists its definitions
//! ([`Engine::list`]), lists and clears the cross-run cache ([`Engine::cache_list`],
//! [`Engine::cache_clear`]), exports the editor schemas ([`Engine::schema_export`]) and works out
//! guarded authoring changes and writes them ([`Engine::fix`], [`Engine::skip`],
//! [`Engine::unskip`], [`Engine::write`]). The model and
//! diagnostics types apps need are re-exported here (D108).
//!
//! ```
//! use vc_engine::Engine;
//!
//! let schemas = Engine::schema_export().unwrap();
//! assert_eq!(schemas[0].file_name, "config.schema.json");
//! ```

mod authoring;
mod builder;
mod cache;
mod check;
mod edit;
mod error;
mod failed;
mod fix;
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
mod skip;
mod stored;
mod user;

pub use builder::{Engine, EngineBuilder};
pub use cache::CacheReport;
pub use check::{CheckReport, CheckRequest};
pub use edit::{Changes, FileChange};
pub use error::EngineError;
pub use fix::FixReport;
pub use interrupt::{Interrupter, Interrupts, interrupts};
pub use list::{ListReport, Listed};
pub use plan::PlanReport;
pub use request::{ListKind, ListRequest, PlanRequest, ReportFlags, RunRequest, SelectRequest};
pub use run::RunReport;
pub use schema::SchemaFile;
pub use skip::SkipRequest;
pub use stored::{HistoryReport, ReportFiles, ReportRequest, StoredRun};
pub use vc_diagnostics::{Diagnostics, DiagnosticsError, JsonRenderer, Render, Summary, TextRenderer};
pub use vc_model::{FileKind, SourceFiles};
