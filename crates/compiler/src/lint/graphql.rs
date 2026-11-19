//! What the `graphql` lints share (D346, D356): each `graphql` api whose document parses, and the
//! operation its `operationName` picks. The documents are parsed once per compile
//! ([`crate::graphql::Documents`]); one that does not parse is V1014 before the build, and a
//! document file the compile was not given is left alone.

use std::ops::Range;

use indexmap::IndexMap;
use vc_model::SourceSpan;
use vc_parser::{GraphqlOperation, Node};

use super::LintCx;
use crate::graphql::{self, OPERATION_NAME, Origin, Unpicked, Written};
use crate::link::Source;

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
    /// The document, where it is written.
    pub written: Written<'c>,
}

impl Document<'_> {
    /// Where bytes `at` of the document are, when they can be placed exactly.
    pub fn spot(&self, at: &Range<usize>) -> Option<SourceSpan> {
        self.written.spot((self.source, self.query), at)
    }

    /// The operation its `operationName` picks, else its only one, or why it picks none; `None`
    /// when `operationName` is not text, which V2302 reports.
    pub fn pick(&self) -> Option<Result<&GraphqlOperation, Unpicked>> {
        let name = match self.body.get(OPERATION_NAME) {
            Some(name) => Some(name.as_str()?),
            None => None,
        };
        Some(self.written.pick(name))
    }

    /// The operation its `operationName` picks, else its only one.
    pub fn picked(&self) -> Option<&GraphqlOperation> {
        self.pick()?.ok()
    }
}

/// Every `graphql` api whose document parses.
pub fn documents<'c>(cx: &'c LintCx<'_, '_>) -> Vec<Document<'c>> {
    let sources = cx.project.sources.iter();
    let queries = sources.flat_map(|source| graphql::queries(source).into_iter().map(move |api| (source, api)));
    let document = |(source, (api, body, query))| {
        let written = cx.documents.get(source, query)?;
        Some(Document { source, api, body, query, written })
    };
    queries.filter_map(document).collect()
}

/// Each document once, by where it is written, with the count of apis that use it: a `$file`
/// shared by several apis is reported once.
pub fn distinct<'d, 'c>(documents: &'d [Document<'c>]) -> Vec<(&'d Document<'c>, usize)> {
    let mut seen: IndexMap<Origin, (&'d Document<'c>, usize)> = IndexMap::new();
    for document in documents {
        seen.entry(document.written.origin).or_insert((document, 0)).1 += 1;
    }
    seen.into_values().collect()
}
