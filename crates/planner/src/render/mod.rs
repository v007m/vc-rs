//! Renders a plan for people and tools: the plan line, the `vc plan` tree, JSON, DOT and the
//! merged expectations with their origins (`--explain`, D123).

mod dot;
mod explain;
mod json;
mod tree;

use std::fmt::Write as _;

use vc_model::{ActionId, ActionKind, Instance, Plan, Program, Sym};

pub use self::dot::dot;
pub use self::explain::explain;
pub use self::json::json;
pub use self::tree::tree;

/// The one-line summary `vc run` prints first:
/// `12 tests in 3 files · lifecycle: beforeRun admin · beforeAll catalog · beforeEach ×9`.
///
/// The engine appends what only it knows, such as cache hits.
#[must_use]
pub fn plan_line(program: &Program, plan: &Plan) -> String {
    let mut line = format!("{} in {}", count(plan.instance_count(), "test"), count(plan.files.len(), "file"));
    let lifecycle = lifecycle(program, plan);
    if !lifecycle.is_empty() {
        line.push_str(" · lifecycle: ");
        line.push_str(&lifecycle.join(" · "));
    }
    if !plan.skipped.is_empty() {
        let _ = write!(line, " · {} skipped", plan.skipped.len());
    }
    line
}

fn lifecycle(program: &Program, plan: &Plan) -> Vec<String> {
    let mut parts = Vec::new();
    let run: Vec<_> = plan.before_run.iter().map(|planned| action_label(program, planned.action)).collect();
    if !run.is_empty() {
        parts.push(format!("beforeRun {}", run.join(", ")));
    }
    let all: Vec<_> = plan
        .files
        .iter()
        .flat_map(|file| &file.before_all)
        .map(|planned| action_label(program, planned.action))
        .collect();
    if !all.is_empty() {
        parts.push(format!("beforeAll {}", all.join(", ")));
    }
    let each = plan.files.iter().filter(|file| has_before_each(program, file.id)).map(|file| file.instances.len());
    let each: usize = each.sum();
    if each > 0 {
        parts.push(format!("beforeEach ×{each}"));
    }
    parts
}

fn has_before_each(program: &Program, file: vc_model::FileId) -> bool {
    program.test_files.get(&file).is_some_and(|file| !file.lifecycle.before_each.is_empty())
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

/// An interned name, or `?` when the symbol is not from this program.
fn name(program: &Program, sym: Sym) -> &str {
    program.str(sym).unwrap_or("?")
}

/// An action as people know it: its `id`, else what it does.
fn action_label(program: &Program, id: ActionId) -> String {
    let Some(action) = usize::try_from(id.index()).ok().and_then(|at| program.actions.get(at)) else {
        return format!("action {}", id.index());
    };
    if let Some(sym) = action.id {
        return name(program, sym).to_owned();
    }
    match &action.kind {
        ActionKind::Call(api) => format!("call {}", name(program, api.sym())),
        ActionKind::Test(test) => format!("test {}", name(program, test.sym())),
        ActionKind::Function(hook) => format!("function {}#{}", hook.file, hook.export),
        ActionKind::Command(_) => "command".to_owned(),
        ActionKind::Inline(_) => "request".to_owned(),
    }
}

/// `users.crud#rejectsBadEmail [row 2, repeat 3]`, numbered from 1; the repeat is shown only
/// when the test repeats.
fn instance_label(program: &Program, instance: &Instance, repeats: bool) -> String {
    let mut marks = Vec::new();
    if let Some(row) = instance.example_row {
        marks.push(format!("row {}", u64::from(row) + 1));
    }
    if repeats {
        marks.push(format!("repeat {}", u64::from(instance.repeat_index) + 1));
    }
    let test = name(program, instance.test.sym());
    if marks.is_empty() { test.to_owned() } else { format!("{test} [{}]", marks.join(", ")) }
}

/// Whether any instance of `instance`'s test in `instances` is a second or later repeat.
fn repeats(instances: &[Instance], instance: &Instance) -> bool {
    instances.iter().any(|other| other.test == instance.test && other.repeat_index > 0)
}
