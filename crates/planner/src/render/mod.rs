//! Renders a plan for people and tools: the plan line, the `vc plan` tree, JSON, DOT and the
//! merged expectations with their origins (`--explain`, D123).

mod dot;
mod explain;
mod json;
mod matcher;
mod tree;

use std::collections::HashSet;
use std::fmt::Write as _;

use vc_model::{ActionId, ActionKind, Instance, InstanceLabels, Plan, PlannedFile, Program, Sym};

pub use self::dot::dot;
pub use self::explain::{check_lines, explain, shown_check_lines};
pub use self::json::json;
pub use self::tree::tree;

/// The one-line summary `vc run` prints first:
/// `4 tests in 3 files · lifecycle: beforeRun admin · beforeAll catalog · beforeEach ×9 calls`.
///
/// It counts tests, not instances: an example row or a repeat adds `beforeEach` calls, not tests.
/// The engine appends what only it knows, such as the profiles and cache hits.
#[must_use]
pub fn plan_line(program: &Program, plan: &Plan) -> String {
    let mut line = format!("{} in {}", count(test_count(plan), "test"), count(plan.files.len(), "file"));
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
    let each: usize = plan.files.iter().map(|file| before_each_calls(program, file)).sum();
    if each > 0 {
        parts.push(format!("beforeEach ×{each} calls"));
    }
    parts
}

/// Distinct tests in the plan; example rows and repeats of one test count once.
fn test_count(plan: &Plan) -> usize {
    let tests: HashSet<_> = plan.files.iter().flat_map(|file| &file.instances).map(|instance| instance.test).collect();
    tests.len()
}

/// How many times the file's `beforeEach` actions run: once per action per instance.
fn before_each_calls(program: &Program, file: &PlannedFile) -> usize {
    let actions = program.test_files.get(&file.id).map_or(0, |test_file| test_file.lifecycle.before_each.len());
    actions.saturating_mul(file.instances.len())
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

/// An interned name, or `?` when the symbol is not from this program.
fn name(program: &Program, sym: Sym) -> &str {
    program.str(sym).unwrap_or("?")
}

/// An action as people know it: its `id`, else what it does: `call users#create`.
#[must_use]
pub fn action_label(program: &Program, id: ActionId) -> String {
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

/// `users.crud#rejectsBadEmail › missing @ [repeat 3]`, as the run's reporters label it (D316).
fn instance_label(program: &Program, labels: &InstanceLabels, instance: &Instance) -> String {
    format!("{}{}", name(program, instance.test.sym()), labels.suffix(instance.test, Some(instance.id)))
}
