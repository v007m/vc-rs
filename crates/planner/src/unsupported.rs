//! Action kinds this milestone cannot run, refused at plan time. The executor's missing-executor
//! check stays as the backstop. M4 deletes the entries here when hooks and commands arrive.

use vc_model::{Action, ActionId, ActionKind, Plan, Program};

use crate::PlanError;
use crate::visit::{Visit, Walker};

/// A kind that cannot run yet: its YAML key, and how to recognise it.
struct Unsupported {
    key: &'static str,
    is: fn(&ActionKind) -> bool,
}

const UNSUPPORTED: &[Unsupported] = &[
    Unsupported { key: "function", is: |kind| matches!(kind, ActionKind::Function(_)) },
    Unsupported { key: "command", is: |kind| matches!(kind, ActionKind::Command(_)) },
];

/// The one place a [`PlanError::NotSupportedYet`] is made.
const fn not_supported_yet(key: &'static str, action: ActionId) -> PlanError {
    PlanError::NotSupportedYet { kind: key, action }
}

/// Fails on the first planned action, in run order, whose kind cannot run yet.
pub fn check(program: &Program, plan: &Plan) -> Result<(), PlanError> {
    let mut walker = Walker::new(program, First::default());
    walker.actions(&plan.before_run.iter().map(|planned| planned.action).collect::<Vec<_>>());
    for file in &plan.files {
        walker.actions(&file.before_all.iter().map(|planned| planned.action).collect::<Vec<_>>());
        for instance in &file.instances {
            if let Some(test) = program.tests.get(&instance.test) {
                walker.instance(test);
            }
        }
        walker.actions(&file.after_all);
    }
    walker.actions(&plan.after_run);
    walker.into_visitor().found.map_or(Ok(()), |(key, action)| Err(not_supported_yet(key, action)))
}

/// Remembers the first unsupported action it is shown.
#[derive(Default)]
struct First {
    found: Option<(&'static str, ActionId)>,
}

impl Visit for First {
    fn action(&mut self, id: ActionId, action: &Action) {
        if self.found.is_some() {
            return;
        }
        let kind = UNSUPPORTED.iter().find(|entry| (entry.is)(&action.kind));
        self.found = kind.map(|entry| (entry.key, id));
    }
}

#[cfg(test)]
mod tests {
    use vc_model::{ActionId, ActionKind, HookRef, Instance, InstanceId, Needs, Plan, PlannedAction, PlannedFile};

    use super::check;
    use crate::PlanError;
    use crate::testing::{Fixture, read};

    /// A plan of one instance of `a`, with an action of `kind` planned as `beforeRun`.
    fn planned(fixture: &mut Fixture, kind: ActionKind) -> Plan {
        let before = fixture.action(kind, None);
        let test = fixture.test("a", &[]);
        let instance =
            Instance { id: InstanceId::new(0), test, example_row: None, repeat_index: 0, needs: Needs::default() };
        let file =
            PlannedFile { id: fixture.file, before_all: Vec::new(), instances: vec![instance], after_all: Vec::new() };
        let before_run = vec![PlannedAction { action: before, needs: Needs::default() }];
        Plan { before_run, files: vec![file], after_run: Vec::new(), skipped: Vec::new() }
    }

    #[test]
    fn refuses_the_first_function_or_command_in_run_order() {
        let mut fixture = Fixture::new();
        let plan = planned(&mut fixture, ActionKind::Function(HookRef { file: "a.ts".into(), export: "f".into() }));
        fixture.action_mut(ActionId::new(1)).kind = ActionKind::Command(read("warm"));
        let error = check(&fixture.program, &plan).err();
        assert_eq!(error, Some(PlanError::NotSupportedYet { kind: "function", action: ActionId::new(0) }));
    }

    #[test]
    fn finds_a_command_inside_an_instance_and_passes_calls() {
        let mut fixture = Fixture::new();
        let call = ActionKind::Call(fixture.api);
        let plan = planned(&mut fixture, call);
        assert_eq!(check(&fixture.program, &plan), Ok(()));
        fixture.action_mut(ActionId::new(1)).kind = ActionKind::Command(read("warm"));
        let error = check(&fixture.program, &plan);
        assert_eq!(error, Err(PlanError::NotSupportedYet { kind: "command", action: ActionId::new(1) }));
    }
}
