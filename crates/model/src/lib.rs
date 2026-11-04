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

mod api;
mod builder;
mod config;
mod date;
mod defs;
mod error;
mod event;
mod exchange;
mod expect;
mod files;
mod ids;
mod json_text;
mod outcome;
mod plan;
mod program;
mod source;
mod span;
mod strings;
mod template;
mod test;
mod units;

pub use api::{Api, ApiGroup, BodySpec, PartSpec};
pub use builder::ProgramBuilder;
pub use config::{
    AuthDef, Config, Dataset, Defaults, GeneratorDef, HttpRetry, LoadScenario, LoadStage, MixEntry, Profile,
};
pub use date::Date;
pub use defs::{
    AuthRef, CacheSpec, Examples, HookRef, Lifecycle, Loop, Method, Owner, Poll, RequestHooks, Retry, RunBlocks,
    SaveSpec, Skip, Tag, VarDefs,
};
pub use error::ModelError;
pub use event::{Event, Feed, Sequenced};
pub use exchange::{ConnectionInfo, Exchange, HttpResponse, Protocol, RenderedRequest, Timing};
pub use expect::{Check, CheckKind, CheckResult, Expect, Matcher, MatcherArg, MatcherCall, Origin};
pub use files::SourceFiles;
pub use ids::{ActionId, ApiId, AuthId, DatasetId, FileId, ProfileId, SourceFileId, TestId, VarName};
pub use outcome::{ActionOutcome, CleanupRecord, Failure, OwnerKey, RunStatus, TestResult, TestStatus, Totals};
pub use plan::{Instance, InstanceId, Needs, Plan, PlannedAction, PlannedFile, SkippedTest};
pub use program::Program;
pub use source::{ContentHash, FileKind, SourceFile, SourcePath};
pub use span::{Pos, SourceSpan};
pub use strings::{StringTable, Sym};
pub use template::{
    CelProgram, CelSource, Namespace, Part, Path, PipeCall, Reference, ScalarSource, ScalarStyle, Segment,
    StatusPattern, Template, TemplateValue,
};
pub use test::{Action, ActionKind, InlineRequest, Test, TestBody, TestFile};
pub use units::{Duration, Size};
