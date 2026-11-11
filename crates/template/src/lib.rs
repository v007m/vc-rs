//! Templates, pipes and the CEL wrapper.
//!
//! [`parse`] turns a YAML scalar's text into a [`vc_model::Template`] at compile time:
//!
//! ```
//! use vc_model::{Pos, ScalarSource, ScalarStyle, SourceFileId, SourceSpan, StringTable};
//!
//! let text = "/users/${var.userId | trim}";
//! let end = Pos::new(3, 11 + 27, 40 + text.len());
//! let span = SourceSpan::new(SourceFileId::new(0), Pos::new(3, 11, 40), end);
//! let source = ScalarSource { span, style: ScalarStyle::Plain };
//! let template = vc_template::parse(text, source, &mut StringTable::new())?;
//! assert_eq!(template.parts.len(), 2);
//! assert!(!template.preserves_type());
//! # Ok::<(), vc_template::TemplateError>(())
//! ```
//!
//! [`PipeRegistry`] checks pipes, [`Roots`] checks where a root may be used (D125), [`ROW_ROOTS`]
//! are what a row source read before the run may read, and
//! [`parse_duration`], [`parse_size`], [`parse_rate`] and [`parse_status`] read units and statuses.
//!
//! [`CelCompiler::check`] checks a CEL expression into a [`vc_model::CelProgram`] at compile
//! time; [`CelCompiler::prepare`] turns it into an [`Expr`] evaluated against an [`Activation`].
//!
//! At run time a [`Renderer`] renders templates and template values against a [`Resolve`],
//! which gives the value of each root; [`select`] follows a path into a value, and [`extend`]
//! applies `$extend` and `$remove`. A failure is a [`RenderError`].

mod builtin_pipes;
mod cel_compile;
mod cel_eval;
mod cel_lex;
mod cel_refs;
mod cel_time;
mod codes;
mod error;
mod expr;
mod fake_fns;
mod fakes;
mod grammar;
mod literals;
mod merge;
mod parse;
mod pipe_fns;
mod pipes;
mod regex_gen;
mod render;
mod roots;
mod rows;
mod select;
mod span_map;
mod time;
mod units;

pub use cel_compile::CelCompiler;
pub use error::{RenderError, TemplateError};
pub use expr::{Activation, DEFAULT_STEP_LIMIT, Expr, ExprError};
pub use fakes::{ArgCheck, FakeCall, FakeCx, FakeDef, FakeError, FakeFn, FakeRegistry};
pub use merge::extend;
pub use parse::{parse, root_name};
pub use pipe_fns::{Chance, PipeArgs, PipeFn};
pub use pipes::{ArgType, PipeDef, PipeRegistry, PipeSignature, ValueType};
pub use render::{Renderer, Resolve, to_text};
pub use roots::Roots;
pub use rows::{ROW_HELP, ROW_ROOTS, row_readable, row_root_label, row_var_unread};
pub use select::{path_text, preview, select};
pub use time::rfc3339;
pub use units::{parse_duration, parse_rate, parse_size, parse_status};
