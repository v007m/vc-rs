//! V2314: the document of a `graphql` api does not fit the schema its file's `graphql.schema`
//! names, by the GraphQL spec's validation rules (D347): an unknown field, with the type's close
//! field as its fix, a missing required argument, a value of the wrong type, a selection on a
//! leaf. Each document is checked once against each schema; a schema file that is not one is
//! V1015, and one the compile was not given is left alone.

use indexmap::IndexSet;
use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{GraphqlMismatch, GraphqlSchema};

use super::graphql::{self, Document};
use super::{Lint, LintCx};
use crate::graphql::Origin;
use crate::graphql_schema;
use crate::suggest::{self, Misspelling};

pub struct GraphqlSchemas;

impl Lint for GraphqlSchemas {
    fn code(&self) -> Code {
        Code::V2314
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let documents = graphql::documents(cx);
        let mut seen: IndexSet<(Origin, String)> = IndexSet::new();
        let mut found = Vec::new();
        for document in &documents {
            let Some((path, schema)) = schema(cx, document) else { continue };
            if seen.insert((document.written.origin, path)) {
                found.extend(schema.check_document(document.written.text).iter().map(|at| unfit(document, at)));
            }
        }
        found
    }
}

/// The schema the file of `document` names, with its path, when it is one.
pub fn schema<'c>(cx: &'c LintCx<'_, '_>, document: &Document<'_>) -> Option<(String, &'c GraphqlSchema)> {
    let path = graphql_schema::path(document.source)?;
    let schema = cx.documents.schema(&path)?.ok()?;
    Some((path, schema))
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
