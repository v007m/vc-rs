//! Cycles through `test:` reuse (V2205): a test that, directly or through other tests, reuses
//! itself.

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_parser::Node;

use super::project::{Project, TEST_BLOCKS, list};
use super::saves::reused;

/// One `test:` reuse: the test reused and the value naming it.
struct Edge<'a> {
    to: String,
    node: &'a Node,
}

/// How far the walk has got with a test.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Open,
    Done,
}

/// Reports every cycle once, at the `test:` that closes it.
pub fn check(project: &Project<'_>, diagnostics: &mut Diagnostics) {
    let edges = edges(project);
    let mut ids: Vec<&String> = edges.keys().collect();
    ids.sort_unstable();
    let mut walk = Walk { edges: &edges, marks: HashMap::new(), path: Vec::new(), found: Diagnostics::new() };
    for id in ids {
        walk.visit(id);
    }
    diagnostics.extend(walk.found);
}

/// Every test's reuses, in its `before`, `steps` and `after`: what runs when it is reused.
fn edges<'a>(project: &Project<'a>) -> HashMap<String, Vec<Edge<'a>>> {
    let mut edges = HashMap::new();
    for (full, item) in &project.tests {
        let source = project.source(*item);
        let mine = TEST_BLOCKS.iter().flat_map(|key| list(item.node, key));
        let found = mine.filter_map(|action| {
            let (to, _) = reused(project, (source, action))?;
            Some(Edge { to, node: action.get("test")? })
        });
        edges.insert(full.clone(), found.collect());
    }
    edges
}

struct Walk<'g, 'a> {
    edges: &'g HashMap<String, Vec<Edge<'a>>>,
    marks: HashMap<&'g str, Mark>,
    path: Vec<&'g str>,
    found: Diagnostics,
}

impl<'g> Walk<'g, '_> {
    fn visit(&mut self, id: &'g str) {
        if self.marks.contains_key(id) {
            return;
        }
        self.marks.insert(id, Mark::Open);
        self.path.push(id);
        let edges = self.edges;
        for edge in edges.get(id).map(Vec::as_slice).unwrap_or_default() {
            match self.marks.get(edge.to.as_str()) {
                Some(Mark::Open) => self.found.push(self.cycle(edge)),
                Some(Mark::Done) => {}
                None => self.visit(&edge.to),
            }
        }
        self.path.pop();
        self.marks.insert(id, Mark::Done);
    }

    /// V2205 for the cycle `edge` closes back into the open path.
    fn cycle(&self, edge: &Edge<'_>) -> Diagnostic {
        let start = self.path.iter().position(|id| *id == edge.to).unwrap_or_default();
        let mut chain: Vec<&str> = self.path[start..].to_vec();
        chain.push(&edge.to);
        let message = format!("`test:` reuse forms a cycle: {}", chain.join(" → "));
        let diagnostic = Diagnostic::new(Code::V2205, edge.node.span, message).label("this reuse closes the cycle");
        diagnostic
            .help("a test cannot reuse itself, directly or through other tests; move the shared steps into one test")
    }
}
