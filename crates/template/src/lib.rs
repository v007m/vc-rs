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
//! [`PipeRegistry`] checks pipes, [`Roots`] checks where a root may be used (D125), and
//! [`parse_duration`], [`parse_size`] and [`parse_status`] read units and statuses.
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
mod codes;
mod error;
mod expr;
mod grammar;
mod literals;
mod merge;
mod parse;
mod pipe_fns;
mod pipes;
mod render;
mod roots;
mod select;
mod span_map;
mod units;

pub use cel_compile::CelCompiler;
pub use error::{RenderError, TemplateError};
pub use expr::{Activation, DEFAULT_STEP_LIMIT, Expr, ExprError};
pub use merge::extend;
pub use parse::{parse, root_name};
pub use pipe_fns::{Chance, PipeArgs, PipeFn};
pub use pipes::{ArgType, PipeDef, PipeRegistry, PipeSignature, ValueType};
pub use render::{Renderer, Resolve, to_text};
pub use roots::Roots;
pub use select::{path_text, preview, select};
pub use units::{parse_duration, parse_size, parse_status};
