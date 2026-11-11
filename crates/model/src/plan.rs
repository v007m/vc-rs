use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ActionId, FileId, TestId};

/// A test instance, as its position in the plan: one test, example row and repeat (D103).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(u32);

impl InstanceId {
    /// Wraps the instance's position in the plan.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The instance's position in the plan.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// The shared lifecycle actions (`beforeRun`, `beforeAll`) something depends on, from the
/// planner's needs analysis (D122). When one of them fails, the dependant is blocked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Needs(BTreeSet<ActionId>);

impl Needs {
    /// Whether `action` is needed.
    #[must_use]
    pub fn contains(&self, action: ActionId) -> bool {
        self.0.contains(&action)
    }

    /// The needed actions, lowest ID first.
    pub fn iter(&self) -> impl Iterator<Item = ActionId> + '_ {
        self.0.iter().copied()
    }
}

impl FromIterator<ActionId> for Needs {
    fn from_iter<I: IntoIterator<Item = ActionId>>(actions: I) -> Self {
        Self(actions.into_iter().collect())
    }
}

/// A shared lifecycle action in run order, with the earlier shared actions it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct PlannedAction {
    /// The action.
    pub action: ActionId,
    /// Earlier shared actions whose saved values it reads.
    pub needs: Needs,
}

/// One test instance to run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Instance {
    /// Its position in the plan.
    pub id: InstanceId,
    /// The test.
    pub test: TestId,
    /// The `examples` row, from 0, when the test has examples.
    pub example_row: Option<u32>,
    /// Which `repeat` this is, from 0.
    pub repeat_index: u32,
    /// The shared lifecycle actions the instance reads saved values from.
    pub needs: Needs,
}

/// One test file to run: its `beforeAll`, instances and `afterAll`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct PlannedFile {
    /// The test file.
    pub id: FileId,
    /// `beforeAll`, run in order before any instance starts (D122).
    pub before_all: Vec<PlannedAction>,
    /// The selected instances, in run order.
    pub instances: Vec<Instance>,
    /// `afterAll`, run after the last instance.
    pub after_all: Vec<ActionId>,
    /// The file's selected tests that are skipped, also in [`Plan::skipped`]: reported as the
    /// file starts and counted in its totals. A skipped test of a file with nothing to run is
    /// reported as the run starts.
    pub skipped: Vec<SkippedTest>,
}

/// A selected test that will not run, with the reason shown in reports.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SkippedTest {
    /// The test.
    pub test: TestId,
    /// `skip.reason`.
    pub reason: String,
}

/// What a run will do, made by the planner from the program and the selection (D103).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Plan {
    /// `beforeRun`, run in order before any file starts (D122).
    pub before_run: Vec<PlannedAction>,
    /// The files, in plan order.
    pub files: Vec<PlannedFile>,
    /// `afterRun`, run after the last file.
    pub after_run: Vec<ActionId>,
    /// Selected tests that are skipped.
    pub skipped: Vec<SkippedTest>,
    /// The `name` of each example row that gives one, by test and row index from 0: the
    /// instance's label in reports, `users.crud#rejectsBadEmail › missing @`.
    pub row_names: BTreeMap<(TestId, u32), String>,
}

impl Plan {
    /// How many instances the plan runs.
    #[must_use]
    pub fn instance_count(&self) -> usize {
        self.files.iter().map(|file| file.instances.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::{Instance, InstanceId, Needs, Plan, PlannedFile};
    use crate::{ActionId, FileId, StringTable, TestId};

    #[test]
    fn needs_hold_their_actions_in_order() {
        let needs: Needs = [ActionId::new(3), ActionId::new(1)].into_iter().collect();
        assert!(needs.contains(ActionId::new(1)) && !needs.contains(ActionId::new(2)));
        assert_eq!(needs.iter().collect::<Vec<_>>(), vec![ActionId::new(1), ActionId::new(3)]);
    }

    #[test]
    fn a_plan_counts_its_instances() {
        let mut table = StringTable::new();
        let file = FileId::new(table.intern("users.crud").unwrap());
        let test = TestId::new(table.intern("users.crud#getsUser").unwrap());
        let instance =
            |n| Instance { id: InstanceId::new(n), test, example_row: None, repeat_index: n, needs: Needs::default() };
        let planned = |instances| PlannedFile {
            id: file,
            before_all: Vec::new(),
            instances,
            after_all: Vec::new(),
            skipped: Vec::new(),
        };
        let plan = Plan {
            files: vec![planned(vec![instance(0), instance(1)]), planned(vec![instance(2)])],
            ..Plan::default()
        };
        assert_eq!(plan.instance_count(), 3);
        assert_eq!(InstanceId::new(4).index(), 4);
    }
}
