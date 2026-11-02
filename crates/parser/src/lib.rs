//! YAML and JSON parsing with source spans, and minimal text edits.
//!
//! [`parse`] turns a file's text into a [`Node`] tree in which every node, and every map key,
//! carries its [`SourceSpan`](vc_model::SourceSpan). Nothing outside this crate sees `saphyr`.

mod build;
mod cut;
mod node;
mod parse;
mod positions;
mod problems;
mod scalar;

pub use node::{Entry, Node, Value};
pub use parse::parse;
pub use scalar::{Scalar, ScalarKind, ScalarStyle};
