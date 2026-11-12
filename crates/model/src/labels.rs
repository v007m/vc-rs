//! What follows a test's ID in an instance's label (D316): ` › <row name>` for an example row
//! with a name, then `[row n, repeat m]`, numbered from 1. The row is numbered when it has no
//! name or shares it with another row of the test; the repeat on every instance of a test that
//! repeats. The run's reporters, `vc plan` and the history all label instances this way.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use crate::{InstanceId, Plan, TestId};

/// The labels of a plan's instances.
#[derive(Clone, Debug, Default)]
pub struct InstanceLabels {
    /// Each instance's example row and repeat.
    slots: HashMap<InstanceId, (Option<u32>, u32)>,
    /// The tests that repeat.
    repeated: HashSet<TestId>,
    /// The name of each example row that has one.
    row_names: BTreeMap<(TestId, u32), String>,
    /// The rows whose name another row of the same test gives too.
    shared: HashSet<(TestId, u32)>,
}

impl InstanceLabels {
    /// The labels of `plan`'s instances, its rows named by its `row_names`.
    #[must_use]
    pub fn new(plan: &Plan) -> Self {
        let instances = plan.files.iter().flat_map(|file| &file.instances);
        let slots = instances.clone().map(|i| (i.id, (i.example_row, i.repeat_index))).collect();
        let repeated = instances.filter(|i| i.repeat_index > 0).map(|i| i.test).collect();
        let shared = shared_names(&plan.row_names);
        Self { slots, repeated, row_names: plan.row_names.clone(), shared }
    }

    /// The example row and repeat of `instance`, when the plan has it.
    #[must_use]
    pub fn slot(&self, instance: InstanceId) -> Option<(Option<u32>, u32)> {
        self.slots.get(&instance).copied()
    }

    /// What follows `test`'s ID in `instance`'s label: ` › Oslo [repeat 2]`; empty for an
    /// instance of no row that does not repeat, or none at all.
    #[must_use]
    pub fn suffix(&self, test: TestId, instance: Option<InstanceId>) -> String {
        let Some((row, repeat)) = instance.and_then(|id| self.slot(id)) else { return String::new() };
        let named = row.and_then(|row| self.row_names.get(&(test, row)));
        let mut out = named.map(|name| format!(" › {name}")).unwrap_or_default();
        let numbered = row.filter(|r| named.is_none() || self.shared.contains(&(test, *r)));
        out.push_str(&instance_marks(numbered, self.repeated.contains(&test).then_some(repeat)));
        out
    }
}

/// ` [row 2, repeat 3]` for row and repeat indices from 0, or the part of it that is given;
/// empty when neither is.
#[must_use]
pub fn instance_marks(row: Option<u32>, repeat: Option<u32>) -> String {
    let row = row.map(|r| format!("row {}", u64::from(r) + 1));
    let repeat = repeat.map(|r| format!("repeat {}", u64::from(r) + 1));
    let marks: Vec<String> = row.into_iter().chain(repeat).collect();
    let mut out = String::new();
    if !marks.is_empty() {
        let _ = write!(out, " [{}]", marks.join(", "));
    }
    out
}

/// The rows of `names` whose `name` another row of the same test gives too.
fn shared_names(names: &BTreeMap<(TestId, u32), String>) -> HashSet<(TestId, u32)> {
    let mut counts: HashMap<(TestId, &str), usize> = HashMap::new();
    for (&(test, _), name) in names {
        *counts.entry((test, name.as_str())).or_default() += 1;
    }
    let shared = |test: TestId, name: &String| counts.get(&(test, name.as_str())).is_some_and(|&count| count > 1);
    names.iter().filter(|&(&(test, _), name)| shared(test, name)).map(|(&key, _)| key).collect()
}

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
