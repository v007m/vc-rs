//! V2009: the `operationName` of a `graphql` api does not pick one query or mutation of its
//! document (D346, D356).

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{GraphqlDocument, GraphqlOperation, Node, OperationKind};

use super::graphql::{self, Document, OPERATION_NAME, by_name, only};
use super::{Lint, LintCx};
use crate::suggest::{self, Misspelling};

pub struct GraphqlOperations;

impl Lint for GraphqlOperations {
    fn code(&self) -> Code {
        Code::V2009
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        graphql::documents(cx).iter().filter_map(unpicked).collect()
    }
}

/// V2009 when the `operationName` of the api does not pick a query or mutation of its document.
fn unpicked(document: &Document<'_>) -> Option<Diagnostic> {
    let (query, parsed) = (document.query, &document.parsed);
    let named = document.body.get(OPERATION_NAME).and_then(|at| Some((at, at.as_str()?)));
    let Some((at, name)) = named else {
        let Some(one) = only(parsed) else { return Some(several(query, parsed)) };
        return subscribes(one, query);
    };
    let Some(picked) = by_name(parsed, name) else { return Some(unknown(at, name, parsed)) };
    subscribes(picked, at)
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

/// V2009 at a `query` with no operation, or several and no `operationName`.
fn several(query: &Node, document: &GraphqlDocument) -> Diagnostic {
    let count = document.operations.len();
    let message = format!("the document holds {count} operations and no `operationName` picks one");
    let diagnostic = Diagnostic::new(Code::V2009, query.span, message).label("one operation expected");
    let help = match names(document).first() {
        Some(name) if count > 1 => format!("name the one to run under `body`, such as `operationName: {name}`"),
        _ => "write one `query` or `mutation` in the document".to_owned(),
    };
    diagnostic.help(help)
}

/// V2009 at whatever picked a subscription.
fn subscription(at: &Node) -> Diagnostic {
    let message = "a `graphql` api runs a query or a mutation; subscriptions are not supported yet";
    let diagnostic = Diagnostic::new(Code::V2009, at.span, message).label("a subscription");
    diagnostic.help("pick a query or a mutation of the document")
}
