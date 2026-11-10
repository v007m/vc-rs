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
