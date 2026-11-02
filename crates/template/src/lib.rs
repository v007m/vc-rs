//! Templates, pipes and the CEL wrapper.
//!
//! [`parse`] turns a YAML scalar's text into a [`vc_model::Template`] at compile time:
//!
//! ```
//! use vc_model::{Pos, SourceFileId, SourceSpan, StringTable};
//!
//! let text = "/users/${var.userId | trim}";
//! let end = Pos::new(3, 11 + 27, 40 + text.len());
//! let scalar = SourceSpan::new(SourceFileId::new(0), Pos::new(3, 11, 40), end);
//! let template = vc_template::parse(text, scalar, &mut StringTable::new())?;
//! assert_eq!(template.parts.len(), 2);
//! assert!(!template.preserves_type());
//! # Ok::<(), Box<vc_diagnostics::Diagnostic>>(())
//! ```
//!
//! [`PipeRegistry`] checks pipes, [`Roots`] checks where a root may be used (D125), and
//! [`parse_duration`], [`parse_size`] and [`parse_status`] read units and statuses.

mod builtin_pipes;
mod codes;
mod grammar;
mod parse;
mod pipes;
mod roots;
mod span_map;
mod units;

pub use parse::{parse, root_name};
pub use pipes::{ArgType, PipeDef, PipeRegistry, PipeSignature, ValueType};
pub use roots::Roots;
pub use units::{parse_duration, parse_size, parse_status};
