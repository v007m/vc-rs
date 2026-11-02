//! Diagnostics: codes, spans, fixes and rendering.
//!
//! Every problem a user can act on is a [`Diagnostic`] with a stable [`Code`] from the
//! registry of consts on `Code`, built step by step:
//!
//! ```
//! use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
//! use vc_model::{Pos, SourceFileId, SourceSpan};
//!
//! let span = SourceSpan::new(SourceFileId::new(0), Pos::new(4, 11, 52), Pos::new(4, 23, 64));
//! let diagnostic = Diagnostic::new(Code::V2003, span, r#"unknown api "orders#plcae""#)
//!     .label("not defined")
//!     .help("did you mean orders#place?")
//!     .fix(Fix::certain("Replace with orders#place", vec![TextEdit::replace(span, "orders#place")]));
//! assert!(diagnostic.is_error());
//! ```

mod code;
mod collection;
mod diagnostic;
mod error;
mod fix;
mod severity;

pub use code::{Category, Code};
pub use collection::Diagnostics;
pub use diagnostic::{Diagnostic, Label};
pub use error::DiagnosticsError;
pub use fix::{Fix, TextEdit};
pub use severity::Severity;
