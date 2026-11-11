//! Engine facade and composition root: the only crate apps import.
//!
//! [`EngineBuilder`] is the one place that names the concrete adapters; apps get an
//! [`Engine`] and call it. So far the engine checks a project ([`Engine::check`]), plans and
//! runs its tests ([`Engine::plan`], [`Engine::run`]), lists its definitions
//! ([`Engine::list`]), explains a test, api or setting ([`Engine::explain`],
//! [`Engine::config_explain`]), lists and clears the cross-run cache ([`Engine::cache_list`],
//! [`Engine::cache_clear`]), exports the editor schemas ([`Engine::schema_export`]) and works out
//! guarded authoring changes and writes them ([`Engine::fix`], [`Engine::skip`],
//! [`Engine::unskip`], [`Engine::rename`], [`Engine::new_api`], [`Engine::new_api_from_curl`],
//! [`Engine::new_test`], [`Engine::new_auth`], [`Engine::new_file`], [`Engine::fmt`], [`Engine::init`],
//! [`Engine::save_call_as_test`], [`Engine::write`]), opens and saves the web UI editor's files
//! ([`Engine::read_file`], [`Engine::save_file`]), describes the project and its effective
//! settings, masked ([`Engine::project_info`], [`Engine::config_values`]), and imports specs and
//! keeps api files in step with them ([`Engine::import`], [`Engine::sync`]).
//! Agent calls pass the profiles' agent policy and, when it asks, the approval queue
//! ([`Engine::policy_check`], [`Engine::admit`],
//! [`Engine::approvals_list`], [`Engine::approvals_decide`]); an agent's `vc call`, run and
//! definition write are gated so ([`Engine::write_definition`]).
//! A load scenario runs its stages and judges its thresholds ([`Engine::load`]). Plugins are
//! fetched, approved and removed ([`Engine::plugin_fetch`], [`Engine::plugin_approve`],
//! [`Engine::plugin_list`], [`Engine::plugin_remove`]). The model and
//! diagnostics types apps need are re-exported here (D108).
//!
//! ```
//! use vc_engine::Engine;
//!
//! let schemas = Engine::schema_export().unwrap();
//! assert_eq!(schemas[0].file_name, "config.schema.json");
//! ```

mod agent_gate;
mod agent_write;
mod alert_samples;
mod alerts;
mod approvals;
mod authoring;
mod body_files;
mod builder;
mod cache;
mod call;
mod call_report;
mod check;
mod check_alerts;
mod complete;
mod config_explain;
mod config_values;
mod create;
mod create_api;
mod create_auth;
mod create_curl;
mod create_load;
mod create_vars;
mod edit;
mod editor_files;
mod error;
mod examples;
mod explain;
mod failed;
mod fetch;
mod fix;
mod format;
mod history;
mod import;
mod import_homes;
mod import_recorded;
mod import_titles;
mod import_write;
mod init;
mod interrupt;
mod list;
mod list_detail;
mod load;
mod load_live;
mod load_report;
mod migrate;
mod notify;
mod plan;
mod plugin_hosts;
mod plugin_intact;
mod plugin_lock;
mod plugin_manifest;
mod plugins;
mod policy;
mod ports;
mod problems;
mod project;
mod project_info;
mod rename;
mod rename_name;
mod reporting;
mod request;
mod run;
mod run_feed;
#[cfg(test)]
mod sample;
mod save_call;
mod schema;
mod selection;
mod settings;
mod shown;
mod skip;
mod snapshot_guard;
mod staged;
mod stored;
mod sync;
mod sync_apply;
mod sync_plan;
mod sync_rules;
mod sync_snapshot;
mod user;
mod wanted;
mod watch;
mod yaml_out;

pub use agent_write::WriteRequest;
pub use alert_samples::AlertSent;
pub use approvals::{APPROVAL_WAIT, Admission, AgentOutcome};
pub use builder::{Engine, EngineBuilder};
pub use cache::CacheReport;
pub use call_report::{CallReport, SavedValue};
pub use check::{CheckReport, CheckRequest};
pub use complete::CompleteKind;
pub use config_values::{ConfigLayer, ConfigValue, ConfigValues};
pub use create::{Added, NewTest};
pub use create_api::NewApi;
pub use create_auth::{NewAuth, auth_fields};
pub use edit::{Changes, FileChange};
pub use editor_files::{OpenedFile, SaveFile, SavedFile};
pub use error::EngineError;
pub use explain::ExplainReport;
pub use fix::FixReport;
pub use import::{ImportRequest, Keep};
pub use init::INIT_FOLDERS;
pub use interrupt::{Interrupter, Interrupts, interrupts};
pub use list::{ListReport, Listed};
pub use list_detail::{ApiCall, action_call};
pub use load_report::{ErrorKind, LoadBreach, LoadEnding, LoadReport, LoadRequest, LoadWatch};
pub use plan::PlanReport;
pub use plugin_hosts::launch_plugin;
pub use plugins::{PluginListing, PluginOffer, PluginState};
pub use policy::{AgentCall, Permission};
pub use project_info::ProjectInfo;
pub use request::{
    CallRequest, ExplainRequest, ListKind, ListRequest, PlanRequest, ReportFlags, RunRequest, SelectRequest,
};
pub use run::RunReport;
pub use run_feed::{RunFeed, RunWatch};
pub use save_call::CallAsTest;
pub use schema::SchemaFile;
pub use skip::SkipRequest;
pub use stored::{HistoryReport, ReportFiles, ReportRequest, StoredRun, instance_label, started};
pub use sync::{SyncMode, SyncReport, SyncRequest};
pub use sync_plan::SyncSide;
pub use vc_diagnostics::{
    Code, Diagnostic, Diagnostics, DiagnosticsError, Fix, JsonRenderer, Label, Render, Severity, Summary, TextRenderer,
};
pub use vc_interfaces::{
    AgentAction, Approval, ApprovalId, ApprovalRequest, ApprovalStatus, CheckRow, Decided, ExchangeRow, LoggedAction,
    PluginKind, PluginManifest, PluginPermissions, PluginStderr, RunId, RunRow, RunStart, TestRow, TestTrend, Verdict,
};
pub use vc_model::{
    ActionId, Bodies, CheckKind, CheckResult, CleanupRecord, Event, Exchange, Failure, FileKind, InstanceId, Location,
    Method, Names, Origin, OwnerKey, Planned, RunStatus, Sequenced, SourceFileId, SourceFiles, SourcePath, SourceSpan,
    TestResult, TestStatus, Totals, check_label, check_parts, cut_body,
};
