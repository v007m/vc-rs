//! Instance expansion: each selected test becomes one instance per example row × repeat, or a
//! skipped entry (D126). Files plan in path order (the compiler
//! sorts its sources) and tests keep file order.

use std::collections::BTreeMap;

use vc_model::{Examples, FileId, Instance, InstanceId, Needs, Program, Skip, SkippedTest, Test, TestFile, TestId};

use crate::select::Pick;
use crate::{PlanError, PlanOptions};

/// An (example row, repeat index) pair.
type Slot = (Option<u32>, u32);

/// The instances to run, by file, and the selected tests that are skipped.
#[derive(Debug, Default)]
pub struct Expanded {
    pub files: Vec<(FileId, Vec<Instance>)>,
    pub skipped: Vec<SkippedTest>,
}

/// Expands the picked tests, numbering instances in plan order.
pub struct Expander<'p> {
    program: &'p Program,
    options: &'p PlanOptions,
    include_skipped: bool,
    next: u32,
}

impl<'p> Expander<'p> {
    pub const fn new(program: &'p Program, options: &'p PlanOptions, include_skipped: bool) -> Self {
        Self { program, options, include_skipped, next: 0 }
    }

    pub fn expand(mut self, picked: &BTreeMap<TestId, Pick>) -> Result<Expanded, PlanError> {
        let mut expanded = Expanded::default();
        for file in self.program.test_files.values() {
            let instances = self.file(file, picked, &mut expanded.skipped)?;
            if !instances.is_empty() {
                expanded.files.push((file.id, instances));
            }
        }
        Ok(expanded)
    }

    fn file(
        &mut self,
        file: &TestFile,
        picked: &BTreeMap<TestId, Pick>,
        skipped: &mut Vec<SkippedTest>,
    ) -> Result<Vec<Instance>, PlanError> {
        let mut instances = Vec::new();
        let tests = file.tests.iter().filter_map(|id| Some((self.program.tests.get(id)?, picked.get(id)?)));
        for (test, pick) in tests.filter(|(test, _)| test.standalone) {
            match self.skip(file, test) {
                Some(reason) => skipped.push(SkippedTest { test: test.id, reason }),
                None => instances.extend(self.instances(test, pick)?),
            }
        }
        Ok(instances)
    }

    /// The reason `test` is skipped, from its file's `skip` or its own; `None` when it runs.
    fn skip(&self, file: &TestFile, test: &Test) -> Option<String> {
        if self.include_skipped {
            return None;
        }
        let active = |skip: &&Skip| !self.expired(skip);
        file.skip.as_ref().filter(active).or_else(|| test.skip.as_ref().filter(active)).map(|skip| skip.reason.clone())
    }

    /// After `until`, the skip no longer holds.
    fn expired(&self, skip: &Skip) -> bool {
        let today = self.options.today.as_deref();
        today.zip(skip.until.as_deref()).is_some_and(|(today, until)| today > until)
    }

    fn instances(&mut self, test: &Test, pick: &Pick) -> Result<Vec<Instance>, PlanError> {
        let all = self.slots(test)?;
        let chosen = match pick {
            Pick::All => all,
            Pick::Ordinals(ordinals) => {
                ordinals.iter().map(|(n, selector)| nth(&all, *n, selector)).collect::<Result<_, _>>()?
            }
        };
        Ok(chosen
            .into_iter()
            .map(|(example_row, repeat_index)| self.instance(test.id, example_row, repeat_index))
            .collect())
    }

    fn instance(&mut self, test: TestId, example_row: Option<u32>, repeat_index: u32) -> Instance {
        let id = InstanceId::new(self.next);
        self.next += 1;
        Instance { id, test, example_row, repeat_index, needs: Needs::default() }
    }

    /// Every (example row, repeat) of `test`, rows outermost.
    fn slots(&self, test: &Test) -> Result<Vec<Slot>, PlanError> {
        let repeat = self.options.repeat.unwrap_or(test.repeat);
        let Some(rows) = self.rows(test)? else {
            return Ok((0..repeat).map(|index| (None, index)).collect());
        };
        Ok((0..rows).flat_map(|row| (0..repeat).map(move |index| (Some(row), index))).collect())
    }

    /// How many example rows `test` has; `None` when it has no `examples`.
    fn rows(&self, test: &Test) -> Result<Option<u32>, PlanError> {
        match &test.examples {
            None => Ok(None),
            Some(Examples::Rows(rows)) => Ok(Some(u32::try_from(rows.len()).unwrap_or(u32::MAX))),
            Some(Examples::File(_) | Examples::Template(_)) => {
                let loaded = self.options.example_rows.get(&test.id).copied();
                let name = || self.program.str(test.id.sym()).unwrap_or_default().to_owned();
                loaded.map(Some).ok_or_else(|| PlanError::RowsNotLoaded { test: name() })
            }
        }
    }
}

/// The `n`th (from 1) of `slots`.
fn nth(slots: &[(Option<u32>, u32)], n: u32, selector: &str) -> Result<(Option<u32>, u32), PlanError> {
    let at = n.checked_sub(1).and_then(|at| usize::try_from(at).ok());
    let count = u32::try_from(slots.len()).unwrap_or(u32::MAX);
    at.and_then(|at| slots.get(at).copied())
        .ok_or_else(|| PlanError::NoSuchInstance { selector: selector.to_owned(), count })
}
