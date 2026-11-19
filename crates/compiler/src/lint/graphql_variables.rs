//! V2108: the `body.variables` of a `graphql` api leave out a variable its operation needs (non-null,
//! with no default), an error; or give one the operation does not declare, a warning, since a
//! GraphQL server ignores it (D346, D356, D357). `variables` written as one template is left
//! alone, since its keys are known only at run time.

use vc_diagnostics::{Code, Diagnostic, Severity};
use vc_model::SourceSpan;
use vc_parser::{Entry, GraphqlOperation, Node, Value};

use super::graphql::{self, Document};
use super::{Lint, LintCx};
use crate::graphql::VARIABLES;
use crate::suggest::{self, Misspelling};

pub struct GraphqlVariables;

impl Lint for GraphqlVariables {
    fn code(&self) -> Code {
        Code::V2108
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        graphql::documents(cx).iter().flat_map(unmatched).collect()
    }
}

/// V2108 for each variable the picked operation needs and is not given, and each value given for
/// a variable it does not declare.
fn unmatched(document: &Document<'_>) -> Vec<Diagnostic> {
    let Some(operation) = document.picked() else { return Vec::new() };
    let given = match document.body.get(VARIABLES).map(|variables| &variables.value) {
        None => &[][..],
        Some(Value::Map(entries)) => entries.as_slice(),
        Some(_) => return Vec::new(),
    };
    let at = key_span(document.body, VARIABLES).or_else(|| key_span(document.api, "body"));
    let at = at.unwrap_or(document.body.span);
    let needed = operation.variables.iter().filter(|variable| variable.required);
    let missing = needed.filter(|variable| !given.iter().any(|entry| entry.key == variable.name));
    let mut found: Vec<Diagnostic> = missing.map(|variable| unset(at, &variable.name)).collect();
    found.extend(given.iter().filter_map(|entry| undeclared(operation, entry)));
    found
}

/// Where `key` is written in the map `node`.
fn key_span(node: &Node, key: &str) -> Option<SourceSpan> {
    node.entries().iter().find(|entry| entry.key == key).map(|entry| entry.key_span)
}

/// V2108 at `at` for the variable `name` that has no value.
fn unset(at: SourceSpan, name: &str) -> Diagnostic {
    let message = format!("the operation needs a value for `${name}`, and `body.variables` gives none");
    let diagnostic = Diagnostic::new(Code::V2108, at, message).label(format!("no `{name}` here"));
    diagnostic.help(format!("add `{name}: …` under `body.variables`, or give `${name}` a default in the document"))
}

/// V2108, a warning, at a value given for a variable `operation` does not declare, with the close
/// name as its fix (D357).
fn undeclared(operation: &GraphqlOperation, entry: &Entry) -> Option<Diagnostic> {
    let declared: Vec<&str> = operation.variables.iter().map(|variable| variable.name.as_str()).collect();
    if declared.contains(&entry.key.as_str()) {
        return None;
    }
    let message = format!("the operation declares no variable `${}`", entry.key);
    let mut diagnostic = Diagnostic::new(Code::V2108, entry.key_span, message)
        .label("not a variable of the operation")
        .note("a GraphQL server ignores a variable the operation does not declare");
    diagnostic.severity = Severity::Warning;
    if declared.is_empty() {
        return Some(diagnostic.help("remove it, or declare it in the operation, such as `query Q($id: ID!)`"));
    }
    Some(suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: &declared }))
}
