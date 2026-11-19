//! What the `graphql` lints share (D346, D356): each `graphql` api whose document parses, and the
//! operation its `operationName` picks. A document that does not parse is V1014 before the
//! build, and a document file the compile was not given is left alone.

use std::ops::Range;

use vc_model::SourceSpan;
use vc_parser::{GraphqlDocument, GraphqlOperation, Node};

use super::LintCx;
use crate::graphql::{self, Written};
use crate::link::Source;

/// The `body` key that names the operation.
pub const OPERATION_NAME: &str = "operationName";

/// A `graphql` api and its parsed document.
pub struct Document<'c> {
    /// The api file.
    pub source: &'c Source<'c>,
    /// The api.
    pub api: &'c Node,
    /// Its `body`.
    pub body: &'c Node,
    /// The body's `query`.
    pub query: &'c Node,
    /// Where the document is written.
    pub written: Written<'c>,
    /// What it holds.
    pub parsed: GraphqlDocument,
}

impl Document<'_> {
    /// Where bytes `at` of the document are, when they can be placed exactly.
    pub fn spot(&self, at: &Range<usize>) -> Option<SourceSpan> {
        self.written.spot((self.source, self.query), at)
    }

    /// The operation its `operationName` names, else its only one.
    pub fn picked(&self) -> Option<&GraphqlOperation> {
        match self.body.get(OPERATION_NAME) {
            Some(name) => by_name(&self.parsed, name.as_str()?),
            None => only(&self.parsed),
        }
    }
}

/// Every `graphql` api whose document parses.
pub fn documents<'c>(cx: &'c LintCx<'_, '_>) -> Vec<Document<'c>> {
    let sources = cx.project.sources.iter();
    let queries = sources.flat_map(|source| graphql::queries(source).into_iter().map(move |api| (source, api)));
    queries.filter_map(|(source, (api, body, query))| document(cx, source, (api, body, query))).collect()
}

/// The document of the api `api`, when it parses.
fn document<'c>(
    cx: &'c LintCx<'_, '_>,
    source: &'c Source<'c>,
    (api, body, query): (&'c Node, &'c Node, &'c Node),
) -> Option<Document<'c>> {
    let written = Written::of(&cx.program.files, (source, query))?;
    let parsed = vc_parser::parse_document(written.text()).ok()?;
    Some(Document { source, api, body, query, written, parsed })
}

/// The operation named `name`.
pub fn by_name<'d>(document: &'d GraphqlDocument, name: &str) -> Option<&'d GraphqlOperation> {
    document.operations.iter().find(|operation| operation.name.as_deref() == Some(name))
}

/// The document's one operation.
pub const fn only(document: &GraphqlDocument) -> Option<&GraphqlOperation> {
    let [one] = document.operations.as_slice() else { return None };
    Some(one)
}
