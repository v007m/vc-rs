//! YAML and JSON parsing with source spans, and minimal text edits.
//!
//! [`parse`] turns a file's text into a [`Node`] tree in which every node, and every map key,
//! carries its [`SourceSpan`](vc_model::SourceSpan). YAML is read from `saphyr`'s events and JSON
//! by a reader of its own; a file's [`Syntax`] comes from its suffix. Nothing outside this crate
//! sees `saphyr`. [`Document`] turns structural changes into
//! [`TextEdit`](vc_diagnostics::TextEdit)s on the original text and checks that the result
//! parses again (D107).

mod build;
mod cut;
mod edit;
mod error;
mod json;
mod json_string;
mod node;
mod parse;
mod positions;
mod problems;
mod scalar;
mod scan;
mod syntax;
mod yaml;

pub use edit::Document;
pub use error::ParserError;
pub use node::{Entry, Node, Value};
pub use parse::parse;
pub use scalar::{Scalar, ScalarKind, ScalarStyle};
pub use syntax::Syntax;
