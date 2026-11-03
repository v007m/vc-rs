//! V3201: a `PUT`, `PATCH` or `DELETE` call sends a value shared from `beforeRun` or the file's
//! `beforeAll`, so one test changes what every other test reads.

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Method, Namespace, Reference, Segment, SourceSpan};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link::{Source, actions, list, method, piece_refs, text};

/// The methods that change a resource.
const WRITES: [Method; 3] = [Method::Put, Method::Patch, Method::Delete];

/// The keys whose values a call sends.
const SENT: [&str; 5] = ["vars", "auth", "headers", "query", "body"];

/// Each shared action's ID, with the block it is in and where the ID is written.
type Shared<'n> = HashMap<&'n str, (&'static str, SourceSpan)>;

/// Reports every shared value a writing call sends.
pub struct SharedWrites;

impl Lint for SharedWrites {
    fn code(&self) -> Code {
        Code::V3201
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let run = cx.project.config().map(|config| shared(config.root, "beforeRun")).unwrap_or_default();
        let mut found = Vec::new();
        for source in cx.test_files() {
            let mut ids = run.clone();
            ids.extend(shared(source.root, "beforeAll"));
            for call in writes(cx, source) {
                found.extend(self.sent(source, call, &ids));
            }
        }
        found
    }
}

/// The calls in a test file that run per test and write: every action outside `beforeAll`, and
/// every test with `call`.
fn writes<'a>(cx: &LintCx<'_, 'a>, source: &Source<'a>) -> Vec<&'a Node> {
    let all: Vec<SourceSpan> = list(source.root, "beforeAll").iter().map(|action| action.span).collect();
    let per_test = actions(source).into_iter().filter(|action| !all.contains(&action.span));
    let tests = list(source.root, "tests").iter().filter(|test| test.get("call").is_some());
    let writes = |call: &&Node| cx.api(source, call).is_some_and(|api| WRITES.contains(&method(api)));
    per_test.chain(tests).filter(writes).collect()
}

impl SharedWrites {
    /// The diagnostics for the shared values the call at `node` sends.
    fn sent(&self, source: &Source<'_>, node: &Node, ids: &Shared<'_>) -> Vec<Diagnostic> {
        let mut refs = Vec::new();
        for value in SENT.iter().filter_map(|key| node.get(key)) {
            source.pieces(value.span, &[]).for_each(|piece| piece_refs(piece, &mut refs));
        }
        let written = text(node, "call").unwrap_or_default();
        let shared = refs.into_iter().filter_map(|reference| Some((reference, ids.get(root(reference)?)?)));
        shared.map(|(reference, (block, at))| self.report(reference, (block, *at), written)).collect()
    }

    fn report(&self, reference: &Reference, (block, at): (&str, SourceSpan), call: &str) -> Diagnostic {
        let message = format!("the call to `{call}` changes a value shared from `{block}`");
        Diagnostic::new(self.code(), reference.span, message)
            .label("shared with every test")
            .secondary(at, format!("shared from `{block}`"))
            .help(
                "one test's change is seen by every test that reads it; make the resource in `before` or `beforeEach`",
            )
    }
}

/// The IDs of the actions in `block` of `root`.
fn shared<'n>(root: &'n Node, block: &'static str) -> Shared<'n> {
    let ids = list(root, block).iter().filter_map(|action| action.get("id"));
    ids.filter_map(|id| Some((id.as_str()?, (block, id.span)))).collect()
}

/// The first name after `var.`, which names a shared action when one has that ID.
fn root(reference: &Reference) -> Option<&str> {
    match (&reference.ns, reference.path.segments.first()?) {
        (Namespace::Var, Segment::Key(name)) => Some(name),
        _ => None,
    }
}
