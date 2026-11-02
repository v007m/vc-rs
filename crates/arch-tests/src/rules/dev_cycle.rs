use std::collections::{BTreeMap, BTreeSet};

use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-DEV-CYCLE";

type Graph<'a> = BTreeMap<&'a str, BTreeSet<&'a str>>;

/// The dependency graph between workspace crates, dev-dependencies included, has no cycle.
pub struct DevCycle;

impl Rule for DevCycle {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let graph = graph(workspace);
        let reach: Graph<'_> = graph.keys().map(|node| (*node, reachable(&graph, node))).collect();
        let mut reported = BTreeSet::new();
        let mut found = Vec::new();
        for (node, from_node) in &reach {
            if reported.contains(node) || !from_node.contains(node) {
                continue;
            }
            let cycle: Vec<&str> =
                from_node.iter().copied().filter(|other| reach.get(other).is_some_and(|r| r.contains(node))).collect();
            reported.extend(cycle.iter().copied());
            let manifest = workspace.package(node).map_or("Cargo.toml", |p| p.manifest.as_str());
            found.push(Violation::new(
                ID,
                manifest,
                format!("dependency cycle (dev-dependencies included): {}", cycle.join(", ")),
            ));
        }
        found
    }
}

fn graph(workspace: &Workspace) -> Graph<'_> {
    let mut graph: Graph<'_> = workspace.packages().iter().map(|p| (p.name.as_str(), BTreeSet::new())).collect();
    for (package, dep) in workspace.internal_deps() {
        graph.entry(package.name.as_str()).or_default().insert(dep.name.as_str());
    }
    graph
}

fn reachable<'a>(graph: &Graph<'a>, start: &str) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = graph.get(start).into_iter().flatten().copied().collect();
    while let Some(node) = stack.pop() {
        if seen.insert(node) {
            stack.extend(graph.get(node).into_iter().flatten().copied());
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::DevCycle;
    use crate::DepKind::{Dev, Normal};
    use crate::Rule;
    use crate::fixtures::{package, workspace};

    #[test]
    fn accepts_an_acyclic_graph_with_dev_edges() {
        let ws = workspace(
            vec![
                package("vc-model", &[]),
                package("vc-testkit", &[("vc-model", Normal)]),
                package("vc-planner", &[("vc-model", Normal), ("vc-testkit", Dev)]),
            ],
            vec![],
        );
        assert_eq!(DevCycle.check(&ws), []);
    }

    #[test]
    fn reports_a_cycle_closed_by_a_dev_edge_once() {
        let ws = workspace(
            vec![
                package("vc-interfaces", &[("vc-testkit", Dev)]),
                package("vc-testkit", &[("vc-interfaces", Normal)]),
                package("vc-model", &[]),
            ],
            vec![],
        );
        let found: Vec<String> = DevCycle.check(&ws).iter().map(ToString::to_string).collect();
        assert_eq!(
            found,
            [
                "ARCH-DEV-CYCLE crates/interfaces/Cargo.toml: dependency cycle (dev-dependencies included): vc-interfaces, vc-testkit"
            ]
        );
    }
}
