//! V2109, a warning: the `body.variables` of a `graphql` api give a value for a variable its
//! operation does not declare, which a GraphQL server ignores (D357, D358). The close declared
//! name is its fix.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Entry, GraphqlOperation};

use super::graphql::{self, Document};
use super::graphql_variables::given;
use super::{Lint, LintCx};
use crate::suggest::{self, Misspelling};

pub struct GraphqlUndeclared;

impl Lint for GraphqlUndeclared {
    fn code(&self) -> Code {
        Code::V2109
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        graphql::documents(cx).iter().flat_map(extra).collect()
    }
}

/// V2109 for each value given for a variable the picked operation does not declare.
fn extra(document: &Document<'_>) -> Vec<Diagnostic> {
    let Some((operation, given)) = given(document) else { return Vec::new() };
    given.iter().filter_map(|entry| undeclared(operation, entry)).collect()
}

/// V2109 at a value given for a variable `operation` does not declare, with the close name as its fix.
fn undeclared(operation: &GraphqlOperation, entry: &Entry) -> Option<Diagnostic> {
    let declared: Vec<&str> = operation.variables.iter().map(|variable| variable.name.as_str()).collect();
    if declared.contains(&entry.key.as_str()) {
        return None;
    }
    let message = format!("the operation declares no variable `${}`", entry.key);
    let diagnostic = Diagnostic::new(Code::V2109, entry.key_span, message)
        .label("not a variable of the operation")
        .note("a GraphQL server ignores a variable the operation does not declare");
    if declared.is_empty() {
        return Some(diagnostic.help("remove it, or declare it in the operation, such as `query Q($id: ID!)`"));
    }
    Some(suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: &declared }))
}
