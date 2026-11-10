//! `vc plan --dot`: shared actions and instances as Graphviz nodes, each file a cluster, with an
//! edge from everything to the shared actions it needs.

use std::fmt::Write as _;

use vc_model::{Needs, Plan, Program};

/// The plan as a Graphviz `digraph`.
#[must_use]
pub fn dot(program: &Program, plan: &Plan) -> String {
    let mut out = String::from("digraph plan {\n  rankdir=LR;\n");
    for planned in &plan.before_run {
        let _ = writeln!(
            out,
            "  {}",
            node(&format!("a{}", planned.action.index()), &super::action_label(program, planned.action))
        );
        edges(&mut out, &format!("a{}", planned.action.index()), &planned.needs);
    }
    for (index, file) in plan.files.iter().enumerate() {
        let _ =
            writeln!(out, "  subgraph cluster_{index} {{\n    label={};", quote(super::name(program, file.id.sym())));
        for planned in &file.before_all {
            let line = node(&format!("a{}", planned.action.index()), &super::action_label(program, planned.action));
            let _ = writeln!(out, "    {line}");
        }
        for instance in &file.instances {
            let label = super::instance_label(program, instance, super::repeats(&file.instances, instance));
            let _ = writeln!(out, "    {}", node(&format!("i{}", instance.id.index()), &label));
        }
        out.push_str("  }\n");
        for planned in &file.before_all {
            edges(&mut out, &format!("a{}", planned.action.index()), &planned.needs);
        }
        for instance in &file.instances {
            edges(&mut out, &format!("i{}", instance.id.index()), &instance.needs);
        }
    }
    out.push_str("}\n");
    out
}

fn node(id: &str, label: &str) -> String {
    format!("{id} [label={}];", quote(label))
}

fn edges(out: &mut String, from: &str, needs: &Needs) {
    for action in needs.iter() {
        let _ = writeln!(out, "  {from} -> a{};", action.index());
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}
