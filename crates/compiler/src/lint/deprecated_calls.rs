//! V3203: a call to an api marked `deprecated: true`, which sync sets when the operation leaves
//! the spec.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::FileKind;
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link;
use crate::link::Source;
use crate::shapes;

/// Reports every call to a deprecated api.
pub struct DeprecatedCalls;

impl Lint for DeprecatedCalls {
    fn code(&self) -> Code {
        Code::V3203
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let calls = cx.project.sources.iter().flat_map(|source| calls(source).into_iter().map(move |at| (source, at)));
        calls.filter_map(|(source, call)| self.deprecated(cx, source, call)).collect()
    }
}

impl DeprecatedCalls {
    /// V3203 at `call`; its help says the operation left the spec only for an api that syncs from
    /// one (its file has a `source` or it has an `operationId`).
    fn deprecated(&self, cx: &LintCx<'_, '_>, source: &Source<'_>, call: &Node) -> Option<Diagnostic> {
        let item = cx.project.apis.get(&source.resolve(link::text(call, "call")?))?;
        let api = Some(item.node).filter(|api| link::flag(shapes::api(), api, "deprecated"))?;
        let at = call.entry("call")?.value.span;
        let marked = api.entry("deprecated").map_or(api.span, |entry| entry.key_span);
        let diagnostic = Diagnostic::new(self.code(), at, "this api is deprecated").label("deprecated api");
        let diagnostic = diagnostic.secondary(marked, "marked deprecated here");
        let synced = cx.project.source(*item).root.get("source").is_some() || api.get("operationId").is_some();
        let why = if synced { "the operation left the spec; " } else { "" };
        Some(diagnostic.help(format!("{why}call the api that replaces it, or remove the call")))
    }
}

/// Every node with a `call` in `source`: actions, tests with `call` and apis' `cleanup`.
fn calls<'a>(source: &Source<'a>) -> Vec<&'a Node> {
    let mut nodes = link::actions(source);
    match source.kind {
        FileKind::Test => nodes.extend(link::list(source.root, "tests")),
        FileKind::Api => nodes.extend(link::list(source.root, "apis").iter().filter_map(|api| api.get("cleanup"))),
        FileKind::Config | FileKind::Load => {}
    }
    nodes
}
