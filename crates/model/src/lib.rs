//! IDs, spans, the compiled program, and the Plan and Event types: pure data.
//!
//! Text such as IDs and names is interned once in a [`StringTable`] and passed around as a
//! 4-byte [`Sym`]. Each kind of ID wraps a `Sym` (or an index) in its own newtype, so an
//! [`ApiId`] can never be used where a [`TestId`] is expected:
//!
//! ```compile_fail
//! # use vc_model::{ApiId, StringTable, TestId};
//! let mut table = StringTable::new();
//! let api = ApiId::new(table.intern("users#create").unwrap());
//! let test: TestId = api;
//! ```

mod agents;
mod alerts;
mod api;
mod body_file;
mod builder;
mod config;
mod date;
mod defs;
mod error;
mod event;
mod exchange;
mod expect;
mod files;
mod form;
mod history;
mod hooks;
mod ids;
mod json_text;
mod names;
mod outcome;
mod plan;
mod program;
mod secrets;
mod source;
mod span;
mod strings;
mod template;
mod test;
mod units;
mod verbs;

pub use agents::{AgentPolicy, WriteDefinitions};
pub use alerts::{Alert, AlertChannel, AlertTrigger};
pub use api::{Api, ApiGroup, ApiSource, BodySpec, PartSpec};
pub use body_file::{FILE_KEY, MULTIPART, basename, file_parts, file_path, is_multipart, part_pointer};
pub use builder::ProgramBuilder;
pub use config::{
    AUTH_NEEDS, AuthDef, Config, Dataset, Defaults, GeneratorDef, HttpRetry, LoadScenario, LoadStage, MixEntry,
    PluginRef, Profile, auth_needs,
};
pub use date::Date;
pub use defs::{
    AllowFailure, AuthRef, CacheSpec, Examples, HookRef, Lifecycle, Loop, Method, Owner, Poll, RequestHooks, Retry,
    RunBlocks, SaveSpec, Skip, Tag, VarDefs,
};
pub use error::ModelError;
pub use event::{Event, Feed, Sequenced};
pub use exchange::{ConnectionInfo, Exchange, HttpResponse, Protocol, RenderedRequest, Timing};
pub use expect::{Check, CheckKind, CheckResult, Expect, Matcher, MatcherArg, MatcherCall, Origin};
pub use files::SourceFiles;
pub use form::{form_pairs, form_text};
pub use history::{Bodies, DEFAULT_KEEP, DEFAULT_MAX_BODY, HistoryConfig, Keep, cut_body};
pub use hooks::{HookModule, HookModules, import_paths};
pub use ids::{ActionId, ApiId, AuthId, DatasetId, FileId, ProfileId, SourceFileId, TestId, VarName};
pub use names::{Location, Names, Planned, check_label, check_parts};
pub use outcome::{ActionOutcome, CleanupRecord, Failure, OwnerKey, RunStatus, TestResult, TestStatus, Totals};
pub use plan::{Instance, InstanceId, Needs, Plan, PlannedAction, PlannedFile, SkippedTest};
pub use program::Program;
pub use secrets::{
    DEFAULT_SECRETS_PROVIDER, DEFAULT_SECRETS_SCHEMA, PROFILE_VARIABLE, RedactRule, SecretsConfig, schema_names,
    secret_name,
};
pub use source::{ContentHash, FileKind, SourceFile, SourcePath, hidden_part, resolve_path};
pub use span::{Pos, SourceSpan};
pub use strings::{StringTable, Sym};
pub use template::{
    CelProgram, CelSource, Namespace, Part, Path, PipeCall, Reference, ScalarSource, ScalarStyle, Segment,
    StatusPattern, Template, TemplateValue,
};
pub use test::{Action, ActionKind, InlineRequest, Test, TestBody, TestFile};
pub use units::{Duration, Size};
pub use verbs::{is_third_person, third_person};
