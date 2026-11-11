//! Snapshots of every render of the sample project's full plan. `cargo insta review` updates them
//! after a deliberate change.

use vc_model::{Plan, Program};
use vc_planner::Selection;

use crate::common;

fn full() -> (Program, Plan) {
    let program = common::project();
    let plan = common::planned(&program, &Selection::default());
    (program, plan)
}

#[test]
fn the_tree_shows_lifecycle_needs_and_skips() {
    let (program, plan) = full();
    insta::assert_snapshot!("tree", vc_planner::tree(&program, &plan));
}

#[test]
fn the_json_names_every_action_and_instance() {
    let (program, plan) = full();
    insta::assert_snapshot!("json", serde_json::to_string_pretty(&vc_planner::json(&program, &plan)).unwrap());
}

#[test]
fn the_dot_graph_clusters_files_and_draws_needs() {
    let (program, plan) = full();
    insta::assert_snapshot!("dot", vc_planner::dot(&program, &plan));
}

#[test]
fn explain_shows_where_each_check_came_from() {
    let (program, plan) = full();
    insta::assert_snapshot!("explain", vc_planner::explain(&program, &plan));
}

#[test]
fn the_plan_line_counts_tests_files_and_lifecycle() {
    let (program, plan) = full();
    let lines = [
        vc_planner::plan_line(&program, &plan),
        vc_planner::plan_line(&program, &common::planned(&program, &common::ids(&["orders.checkout#paysOrder"]))),
        vc_planner::plan_line(&program, &common::planned(&program, &common::ids(&["legacy.old"]))),
    ];
    insta::assert_snapshot!("plan_line", lines.join("\n"));
}

/// A project with every lifecycle block, and a test that reads `seed`'s saved `userId`.
const BLOCKS: &str = "beforeAll:
  - id: seed
    call: users#create
beforeEach:
  - call: users#list
afterEach:
  - command: echo each
afterAll:
  - command: echo all
tests:
  - id: listsUsers
    call: users#list
  - id: readsSeed
    call: users#list
    headers: {x-seed: '${var.seed.userId}'}
";

/// The graph's edges, `from -> to` by node label, with `order` or `needs`.
fn edges(graph: &str) -> Vec<String> {
    let labels: std::collections::HashMap<&str, &str> = graph
        .lines()
        .filter_map(|line| line.trim().split_once(" [label=\""))
        .map(|(id, rest)| (id, rest.trim_end_matches("\"];")))
        .collect();
    let named = |id: &str| labels.get(id).copied().unwrap_or(id).to_owned();
    let edge = |line: &str| {
        let (from, rest) = line.trim().split_once(" -> ")?;
        let (to, kind) =
            rest.split_once(' ').map_or_else(|| (rest.trim_end_matches(';'), "needs"), |(to, _)| (to, "order"));
        Some(format!("{} -> {} ({kind})", named(from), named(to)))
    };
    graph.lines().filter_map(edge).collect()
}

#[test]
fn the_dot_graph_has_a_node_per_lifecycle_action_and_edges_for_order_and_needs() {
    let program = common::project_with(&[("services/users/blocks.test.yaml", BLOCKS)]);
    let plan = common::planned(&program, &common::ids(&["users.blocks"]));
    let graph = vc_planner::dot(&program, &plan);
    for label in [
        "beforeRun warm",
        "beforeAll seed",
        "beforeEach call users#list",
        "afterEach command",
        "afterAll command",
        "afterRun bye",
    ] {
        assert!(graph.contains(&format!("[label=\"{label}\"]")), "{label}:\n{graph}");
    }
    let edges = edges(&graph);
    for expected in [
        "beforeRun warm -> beforeRun cold (order)",
        "beforeRun cold -> beforeAll seed (order)",
        "beforeAll seed -> beforeEach call users#list (order)",
        "beforeEach call users#list -> users.blocks#listsUsers (order)",
        "beforeEach call users#list -> users.blocks#readsSeed (order)",
        "users.blocks#readsSeed -> afterEach command (order)",
        "afterEach command -> afterAll command (order)",
        "afterAll command -> afterRun bye (order)",
        "users.blocks#readsSeed -> beforeAll seed (needs)",
    ] {
        assert!(edges.iter().any(|edge| edge == expected), "{expected}:\n{edges:#?}");
    }
    assert_eq!(edges.iter().filter(|edge| edge.ends_with("(needs)")).count(), 1, "{edges:#?}");
}
