//! The tree `vc plan` prints: lifecycle blocks, files and instances, each with what it needs.

use std::fmt::Write as _;

use vc_model::{ActionId, Needs, Plan, PlannedAction, PlannedFile, Program};

use super::{action_label, instance_label, name, repeats};

/// The plan as an indented tree, two spaces a level.
#[must_use]
pub fn tree(program: &Program, plan: &Plan) -> String {
    let mut out = Tree { program, text: String::new() };
    out.planned("beforeRun", &plan.before_run, 0);
    for file in &plan.files {
        out.file(file);
    }
    out.after("afterRun", &plan.after_run, 0);
    if !plan.skipped.is_empty() {
        out.line(0, "skipped");
        for skipped in &plan.skipped {
            out.line(1, &format!("{}: {}", name(program, skipped.test.sym()), skipped.reason));
        }
    }
    out.text
}

struct Tree<'p> {
    program: &'p Program,
    text: String,
}

impl Tree<'_> {
    fn file(&mut self, file: &PlannedFile) {
        self.line(0, name(self.program, file.id.sym()));
        self.planned("beforeAll", &file.before_all, 1);
        for instance in &file.instances {
            let label = instance_label(self.program, instance, repeats(&file.instances, instance));
            let needs = self.needs(&instance.needs);
            self.line(1, &format!("{label}{needs}"));
        }
        self.after("afterAll", &file.after_all, 1);
    }

    fn planned(&mut self, block: &str, actions: &[PlannedAction], depth: usize) {
        if actions.is_empty() {
            return;
        }
        self.line(depth, block);
        for planned in actions {
            let needs = self.needs(&planned.needs);
            self.line(depth + 1, &format!("{}{needs}", action_label(self.program, planned.action)));
        }
    }

    fn after(&mut self, block: &str, actions: &[ActionId], depth: usize) {
        if actions.is_empty() {
            return;
        }
        self.line(depth, block);
        for action in actions {
            self.line(depth + 1, &action_label(self.program, *action));
        }
    }

    /// `  needs: warm, seed`, or nothing.
    fn needs(&self, needs: &Needs) -> String {
        let labels: Vec<_> = needs.iter().map(|action| action_label(self.program, action)).collect();
        if labels.is_empty() { String::new() } else { format!("  needs: {}", labels.join(", ")) }
    }

    fn line(&mut self, depth: usize, text: &str) {
        let _ = writeln!(self.text, "{:indent$}{text}", "", indent = depth * 2);
    }
}
