//! YAML, JSON and CSV parsing with source spans, and minimal text edits.
//!
//! [`parse`] turns a file's text into a [`Node`] tree in which every node, and every map key,
//! carries its [`SourceSpan`](vc_model::SourceSpan). YAML is read from `saphyr`'s events and JSON
//! and CSV datasets by readers of their own; a file's [`Syntax`] comes from its suffix. Nothing outside this crate
//! sees `saphyr`. [`Document`] turns structural changes into
//! [`TextEdit`](vc_diagnostics::TextEdit)s on the original text and checks that the result
//! parses again (D107), or writes a whole file in the house style ([`Document::format`]).

mod build;
mod csv;
mod cut;
mod edit;
mod error;
mod graphql;
mod graphql_schema;
mod introspection;
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

pub use edit::{Document, EditCommand, FormatRules, references};
pub use error::ParserError;
pub use graphql::{GraphqlDocument, GraphqlOperation, GraphqlProblem, GraphqlVariable, OperationKind, parse_document};
pub use graphql_schema::{GraphqlMismatch, GraphqlSchema, parse_schema};
pub use introspection::schema_from_introspection;
pub use node::{Entry, Node, Value};
pub use parse::parse;
pub use scalar::{Scalar, ScalarKind, ScalarStyle, write_text};
pub use syntax::Syntax;
