//! V2108: the `body.variables` of a `graphql` api leave out a variable its operation needs (non-null,
//! with no default; D346, D356, D358). `variables` written as one template is left alone, since
//! its keys are known only at run time.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::SourceSpan;
use vc_parser::{Entry, GraphqlOperation, Node, Value};

use super::graphql::{self, Document};
use super::{Lint, LintCx};
use crate::graphql::VARIABLES;

pub struct GraphqlVariables;

impl Lint for GraphqlVariables {
    fn code(&self) -> Code {
        Code::V2108
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        graphql::documents(cx).iter().flat_map(unmatched).collect()
    }
}

/// The operation the api picks and the values its `body.variables` give, when they are a map or
/// left out.
pub fn given<'d>(document: &'d Document<'_>) -> Option<(&'d GraphqlOperation, &'d [Entry])> {
    let operation = document.picked()?;
    match document.body.get(VARIABLES).map(|variables| &variables.value) {
        None => Some((operation, &[])),
        Some(Value::Map(entries)) => Some((operation, entries.as_slice())),
        Some(_) => None,
    }
}

/// V2108 for each variable the picked operation needs and is not given.
fn unmatched(document: &Document<'_>) -> Vec<Diagnostic> {
    let Some((operation, given)) = given(document) else { return Vec::new() };
    let at = key_span(document.body, VARIABLES).or_else(|| key_span(document.api, "body"));
    let at = at.unwrap_or(document.body.span);
    let needed = operation.variables.iter().filter(|variable| variable.required);
    let missing = needed.filter(|variable| !given.iter().any(|entry| entry.key == variable.name));
    missing.map(|variable| unset(at, &variable.name)).collect()
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
