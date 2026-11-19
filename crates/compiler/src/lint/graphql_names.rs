//! V4010: a `graphql` api's operation has no name (D346, D356). A name shows in the server's logs
//! and traces; the certain fix names it after the api (`getUser` → `GetUser`) when the document
//! holds that one operation and no other api uses it. A `$file` shared by several apis is reported
//! once.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_parser::GraphqlOperation;

use super::graphql::{self, Document};
use super::{Lint, LintCx};
use crate::link;

pub struct GraphqlNames;

impl Lint for GraphqlNames {
    fn code(&self) -> Code {
        Code::V4010
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let documents = graphql::documents(cx);
        let anonymous = graphql::distinct(&documents).into_iter().flat_map(|(document, apis)| {
            let unnamed = document.written.parsed.operations.iter().filter(|operation| operation.name.is_none());
            unnamed.map(move |operation| (document, operation, apis == 1))
        });
        anonymous.map(|(document, operation, own)| unnamed((document, own), operation)).collect()
    }
}

/// V4010 at the keyword of `operation`, or the `{` of a shorthand query; named after the api only
/// when the document is its `own`, used by no other api.
fn unnamed((document, own): (&Document<'_>, bool), operation: &GraphqlOperation) -> Diagnostic {
    let span = document.spot(&operation.head).unwrap_or(document.query.span);
    let name = link::text(document.api, "id").filter(|_| own).and_then(operation_name);
    let help = name.as_ref().map_or_else(
        || "name the operation, such as `query GetUser`".to_owned(),
        |name| format!("name it after the api: `{} {name}`", keyword(document, operation)),
    );
    let diagnostic = Diagnostic::new(Code::V4010, span, "the GraphQL operation has no name")
        .label("an anonymous operation")
        .note("a name shows in the server's logs and traces, and lets `operationName` pick the operation")
        .help(help);
    match name
        .filter(|_| document.written.parsed.operations.len() == 1)
        .and_then(|name| naming(document, operation, &name))
    {
        Some(fix) => diagnostic.fix(fix),
        None => diagnostic,
    }
}

/// The operation's keyword: `query` for a shorthand one.
fn keyword<'d>(document: &'d Document<'_>, operation: &GraphqlOperation) -> &'d str {
    let head = document.written.text.get(operation.head.clone()).unwrap_or("{");
    if head == "{" { "query" } else { head }
}

/// The certain fix that writes `name` after the keyword, or `query <name>` before a shorthand's `{`.
fn naming(document: &Document<'_>, operation: &GraphqlOperation, name: &str) -> Option<Fix> {
    let shorthand = document.written.text.get(operation.head.clone()) == Some("{");
    let (at, text) = if shorthand {
        (operation.head.start, format!("query {name} "))
    } else {
        (operation.head.end, format!(" {name}"))
    };
    let span = document.spot(&(at..at))?;
    Some(Fix::certain(format!("Name it {name}"), vec![TextEdit::replace(span, text)]))
}

/// The api ID `id` as an operation name: `getUser` is `GetUser`; `None` when it is no GraphQL name.
fn operation_name(id: &str) -> Option<String> {
    let mut chars = id.chars();
    let first = chars.next().filter(char::is_ascii_alphabetic)?;
    let valid = id.chars().all(|letter| letter.is_ascii_alphanumeric() || letter == '_');
    valid.then(|| first.to_ascii_uppercase().to_string() + chars.as_str())
}
