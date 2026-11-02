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

mod error;
mod files;
mod ids;
mod program;
mod source;
mod span;
mod strings;
mod template;
mod units;

pub use error::ModelError;
pub use files::SourceFiles;
pub use ids::{ActionId, ApiId, AuthId, DatasetId, FileId, ProfileId, SourceFileId, TestId, VarName};
pub use program::Program;
pub use source::{ContentHash, FileKind, SourceFile, SourcePath};
pub use span::{Pos, SourceSpan};
pub use strings::{StringTable, Sym};
pub use template::{
    CelSource, Namespace, Part, Path, PipeCall, Reference, ScalarSource, ScalarStyle, Segment, StatusPattern, Template,
    TemplateValue,
};
pub use units::{Duration, Size};
