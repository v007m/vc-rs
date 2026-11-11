//! The actions a plan can run, so the engine reads the `$file` bodies of those alone (D272):
//! `beforeRun`, each file's `beforeAll`, instances and `afterAll`, then `afterRun`, following
//! what the walker follows (lifecycle blocks, `test:` reuse, api cleanups, auth steps and cache
//! checks).

use std::collections::BTreeSet;

use vc_model::{Action, ActionId, Plan, PlannedAction, Program, TestId};

use crate::visit::{Visit, Walker};

/// Collects every action the walk reaches.
#[derive(Default)]
struct Reached(BTreeSet<ActionId>);

impl Visit for Reached {
    fn action(&mut self, id: ActionId, _action: &Action) {
        self.0.insert(id);
    }
}

/// Every action `plan` can run, the ones each of them reaches included.
#[must_use]
pub fn bodies(program: &Program, plan: &Plan) -> BTreeSet<ActionId> {
    let mut walker = Walker::new(program, Reached::default());
    walker.actions(&shared(&plan.before_run));
    for file in &plan.files {
        walker.actions(&shared(&file.before_all));
        let tests: BTreeSet<TestId> = file.instances.iter().map(|instance| instance.test).collect();
        tests.iter().filter_map(|test| program.tests.get(test)).for_each(|test| walker.instance(test));
        walker.actions(&file.after_all);
    }
    walker.actions(&plan.after_run);
    walker.into_visitor().0
}

/// Every action `actions` reach, themselves included: for an ad hoc call or a cleanup.
#[must_use]
pub fn bodies_from(program: &Program, actions: &[ActionId]) -> BTreeSet<ActionId> {
    let mut walker = Walker::new(program, Reached::default());
    walker.actions(actions);
    walker.into_visitor().0
}

fn shared(planned: &[PlannedAction]) -> Vec<ActionId> {
    planned.iter().map(|planned| planned.action).collect()
}

#[cfg(test)]
#[path = "bodies_tests.rs"]
mod tests;
