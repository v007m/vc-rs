//! V2314: the document of a `graphql` api does not fit the schema its file's `graphql.schema`
//! names, by the GraphQL spec's validation rules (D347): an unknown field, with the type's close
//! field as its fix, a missing required argument, a value of the wrong type, a selection on a
//! leaf, an unused fragment beside an operation. Each document is checked once against each
//! schema; what V2009 reports of it is left out, and a document is taken as reported when an api
//! that uses it picks no operation, or has an `operationName` that is not text (V2302). A schema file that is not one is V1015, and one
//! the compile was not given is left alone.

use indexmap::IndexMap;
use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{GraphqlMismatch, GraphqlOperations, GraphqlSchema};

use super::graphql::{self, Document};
use super::{Lint, LintCx};
use crate::suggest::{self, Misspelling};

pub struct GraphqlSchemas;

impl Lint for GraphqlSchemas {
    fn code(&self) -> Code {
        Code::V2314
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let documents = graphql::documents(cx);
        let mut checked = IndexMap::new();
        for document in &documents {
            let Some(at) = schema(cx, document) else { continue };
            let reported = !matches!(document.pick(), Some(Ok(_)));
            checked.entry((document.written.origin, at.path)).or_insert((document, at, false)).2 |= reported;
        }
        let unfits = |(document, at, reported): (&Document<'_>, Checked<'_>, bool)| {
            let found = at.schema.check_document(at.operations, reported);
            found.iter().map(|mismatch| unfit(document, mismatch)).collect::<Vec<_>>()
        };
        checked.into_values().flat_map(unfits).collect()
    }
}

/// A document's schema: its path, the schema, and the document parsed against it.
#[derive(Clone, Copy)]
pub struct Checked<'c> {
    pub path: &'c str,
    pub schema: &'c GraphqlSchema,
    pub operations: &'c GraphqlOperations,
}

/// The schema the file of `document` names, when it is one, with the document parsed against it.
pub fn schema<'c>(cx: &'c LintCx<'_, '_>, document: &Document<'_>) -> Option<Checked<'c>> {
    let path = cx.documents.named(document.source.path)?;
    let schema = cx.documents.schema(path)?.ok()?;
    let index = cx.documents.schema_paths().get_index_of(path)?;
    let operations = cx.operations.get((document.written.origin, index))?;
    Some(Checked { path, schema, operations })
}

/// V2314 where `mismatch` is in `document`: at the part at fault, else at its `query`.
fn unfit(document: &Document<'_>, mismatch: &GraphqlMismatch) -> Diagnostic {
    let placed = document.spot(&mismatch.at).filter(|_| !mismatch.at.is_empty());
    let span = placed.unwrap_or(document.query.span);
    let message = format!("the document does not fit the schema: {}", mismatch.message);
    let diagnostic = Diagnostic::new(Code::V2314, span, message).label("not as the schema has it");
    let word = document.written.text.get(mismatch.at.clone()).filter(|_| placed.is_some());
    let known: Vec<&str> = mismatch.fields.iter().map(String::as_str).collect();
    match word.filter(|_| !known.is_empty()) {
        Some(word) => suggest::hint(diagnostic, &Misspelling { word, span, known: &known }),
        None => diagnostic.help("write the document as the schema in `graphql.schema` has it"),
    }
}

#[cfg(test)]
#[path = "graphql_schemas_tests.rs"]
mod tests;
