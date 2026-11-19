//! V2009: the `operationName` of a `graphql` api does not pick one query or mutation of its
//! document, or the document names two operations alike (D346, D356).

use std::collections::HashSet;

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{GraphqlDocument, GraphqlOperation, Node, OperationKind};

use super::graphql::{self, Document};
use super::{Lint, LintCx};
use crate::graphql::{OPERATION_NAME, Unpicked};
use crate::suggest::{self, Misspelling};

pub struct GraphqlOperations;

impl Lint for GraphqlOperations {
    fn code(&self) -> Code {
        Code::V2009
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let documents = graphql::documents(cx);
        let distinct = graphql::distinct(&documents);
        let mut found: Vec<Diagnostic> = distinct.into_iter().filter_map(|(document, _)| repeated(document)).collect();
        found.extend(documents.iter().filter_map(unpicked));
        found
    }
}

/// V2009 at the second operation of a name the document gives two operations.
fn repeated(document: &Document<'_>) -> Option<Diagnostic> {
    let mut seen = HashSet::new();
    let operations = &document.written.parsed.operations;
    let again = operations.iter().find(|operation| operation.name.as_deref().is_some_and(|name| !seen.insert(name)))?;
    let name = again.name.as_deref().unwrap_or_default();
    let span = document.spot(&again.head).unwrap_or(document.query.span);
    let message = format!("the document holds more than one operation named `{name}`");
    let diagnostic = Diagnostic::new(Code::V2009, span, message).label("a second operation of that name");
    Some(diagnostic.help("give each operation its own name; `operationName` picks one by it"))
}

/// V2009 when the `operationName` of the api does not pick a query or mutation of its document.
fn unpicked(document: &Document<'_>) -> Option<Diagnostic> {
    let (query, parsed) = (document.query, document.written.parsed);
    let named = document.body.get(OPERATION_NAME).and_then(|at| Some((at, at.as_str()?)));
    match document.pick()? {
        Ok(operation) => subscribes(operation, named.map_or(query, |(at, _)| at)),
        Err(Unpicked::Unknown) => named.map(|(at, name)| unknown(at, name, parsed)),
        Err(Unpicked::Several) => Some(several(query, parsed)),
        Err(Unpicked::Empty) => Some(empty(query)),
        Err(Unpicked::Twice) => None,
    }
}

/// V2009 at `at` when `operation` is a subscription.
fn subscribes(operation: &GraphqlOperation, at: &Node) -> Option<Diagnostic> {
    (operation.kind == OperationKind::Subscription).then(|| subscription(at))
}

/// The names the document gives its operations.
fn names(document: &GraphqlDocument) -> Vec<&str> {
    document.operations.iter().filter_map(|operation| operation.name.as_deref()).collect()
}

/// V2009 at an `operationName` the document does not define, with the close name as its fix.
fn unknown(at: &Node, name: &str, document: &GraphqlDocument) -> Diagnostic {
    let known = names(document);
    let message = format!("the document defines no operation named `{name}`");
    let diagnostic = Diagnostic::new(Code::V2009, at.span, message).label("not an operation of the document");
    let listed = if known.is_empty() {
        "it names none of its operations".to_owned()
    } else {
        format!("its operations are {}", suggest::quoted(&known))
    };
    let diagnostic = diagnostic.note(listed);
    suggest::hint(diagnostic, &Misspelling { word: name, span: at.span, known: &known })
}

/// V2009 at a `query` with several operations and no `operationName`.
fn several(query: &Node, document: &GraphqlDocument) -> Diagnostic {
    let count = document.operations.len();
    let message = format!("the document holds {count} operations and no `operationName` picks one");
    let diagnostic = Diagnostic::new(Code::V2009, query.span, message).label("one operation expected");
    let help = names(document).first().map_or_else(
        || "name the operations, such as `query GetUser`, and pick one with `operationName`".to_owned(),
        |name| format!("name the one to run under `body`, such as `operationName: {name}`"),
    );
    diagnostic.help(help)
}

/// V2009 at a `query` whose document holds only fragments or type definitions.
fn empty(query: &Node) -> Diagnostic {
    let message = "the document holds no operation (query or mutation)";
    let diagnostic = Diagnostic::new(Code::V2009, query.span, message).label("nothing to run");
    diagnostic.help("write a `query` or `mutation` in the document; fragments and type definitions alone run nothing")
}

/// V2009 at whatever picked a subscription.
fn subscription(at: &Node) -> Diagnostic {
    let message = "a `graphql` api runs a query or a mutation; subscriptions are not supported yet";
    let diagnostic = Diagnostic::new(Code::V2009, at.span, message).label("a subscription");
    diagnostic.help("pick a query or a mutation of the document")
}
