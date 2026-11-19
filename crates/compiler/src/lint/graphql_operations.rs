//! V2009: the `operationName` of a `graphql` api does not pick one query or mutation of its
//! document (D346, D356). A document that does not parse is V1014 before the build, and a
//! document file the compile was not given is left alone.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{GraphqlDocument, GraphqlOperation, Node, OperationKind};

use super::{Lint, LintCx};
use crate::graphql;
use crate::link::Source;
use crate::suggest::{self, Misspelling};

/// The `body` key that names the operation.
const OPERATION_NAME: &str = "operationName";

pub struct GraphqlOperations;

impl Lint for GraphqlOperations {
    fn code(&self) -> Code {
        Code::V2009
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let queries =
            cx.project.sources.iter().flat_map(|source| {
                graphql::queries(source).into_iter().map(move |(body, query)| (source, body, query))
            });
        queries.filter_map(|(source, body, query)| unpicked(cx, source, (body, query))).collect()
    }
}

/// V2009 when the `operationName` of `body` does not pick a query or mutation of its `query`.
fn unpicked(cx: &LintCx<'_, '_>, source: &Source<'_>, (body, query): (&Node, &Node)) -> Option<Diagnostic> {
    let filed = || graphql::file(source, query).and_then(|path| graphql::text(&cx.program.files, &path));
    let document = vc_parser::parse_document(query.as_str().or_else(filed)?).ok()?;
    let named = body.get(OPERATION_NAME).and_then(|at| Some((at, at.as_str()?)));
    let Some((at, name)) = named else {
        let Some(one) = only(&document) else { return Some(several(query, &document)) };
        return subscribes(one, query);
    };
    let Some(picked) = by_name(&document, name) else { return Some(unknown(at, name, &document)) };
    subscribes(picked, at)
}

/// V2009 at `at` when `operation` is a subscription.
fn subscribes(operation: &GraphqlOperation, at: &Node) -> Option<Diagnostic> {
    (operation.kind == OperationKind::Subscription).then(|| subscription(at))
}

/// The operation named `name`.
fn by_name<'d>(document: &'d GraphqlDocument, name: &str) -> Option<&'d GraphqlOperation> {
    document.operations.iter().find(|operation| operation.name.as_deref() == Some(name))
}

/// The document's one operation.
const fn only(document: &GraphqlDocument) -> Option<&GraphqlOperation> {
    let [one] = document.operations.as_slice() else { return None };
    Some(one)
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
