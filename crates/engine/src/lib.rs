//! Engine facade and composition root: the only crate apps import.
//!
//! [`EngineBuilder`] is the one place that names the concrete adapters; apps get an
//! [`Engine`] and call it. So far the engine checks a project ([`Engine::check`]), plans and
//! runs its tests ([`Engine::plan`], [`Engine::run`]), lists its definitions
//! ([`Engine::list`]), explains a test, api or setting ([`Engine::explain`],
//! [`Engine::config_explain`]), lists and clears the cross-run cache ([`Engine::cache_list`],
//! [`Engine::cache_clear`]), exports the editor schemas ([`Engine::schema_export`]) and works out
//! guarded authoring changes and writes them ([`Engine::fix`], [`Engine::skip`],
//! [`Engine::unskip`], [`Engine::rename`], [`Engine::new_api`], [`Engine::new_test`],
//! [`Engine::new_auth`], [`Engine::new_file`], [`Engine::fmt`], [`Engine::init`],
//! [`Engine::save_call_as_test`], [`Engine::write`]). The model and diagnostics types apps need
//! are re-exported here (D108).
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
mod call;
mod check;
mod complete;
mod config_explain;
mod create;
mod create_api;
mod create_auth;
mod edit;
mod error;
mod explain;
mod failed;
mod fix;
mod format;
mod history;
mod init;
mod interrupt;
mod list;
mod plan;
mod ports;
mod problems;
mod project;
mod rename;
mod rename_name;
mod reporting;
mod request;
mod run;
#[cfg(test)]
mod sample;
mod save_call;
mod schema;
mod selection;
mod settings;
mod shown;
mod skip;
mod stored;
mod user;
mod wanted;
mod watch;

pub use builder::{Engine, EngineBuilder};
pub use cache::CacheReport;
pub use call::CallReport;
pub use check::{CheckReport, CheckRequest};
pub use complete::CompleteKind;
pub use create::{Added, NewTest};
pub use create_api::NewApi;
pub use create_auth::{NewAuth, auth_fields};
pub use edit::{Changes, FileChange};
pub use error::EngineError;
pub use explain::ExplainReport;
pub use fix::FixReport;
pub use init::INIT_FOLDERS;
pub use interrupt::{Interrupter, Interrupts, interrupts};
pub use list::{ListReport, Listed};
pub use plan::PlanReport;
pub use request::{
    CallRequest, ExplainRequest, ListKind, ListRequest, PlanRequest, ReportFlags, RunRequest, SelectRequest,
};
pub use run::RunReport;
pub use save_call::CallAsTest;
pub use schema::SchemaFile;
pub use skip::SkipRequest;
pub use stored::{HistoryReport, ReportFiles, ReportRequest, StoredRun};
pub use vc_diagnostics::{Code, Diagnostics, DiagnosticsError, JsonRenderer, Render, Summary, TextRenderer};
pub use vc_model::{FileKind, SourceFiles, SourcePath};
