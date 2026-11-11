//! `plan`: select, expand, then plan the lifecycle and each instance's needs.

use std::collections::HashMap;

use vc_diagnostics::Diagnostic;
use vc_model::{Examples, Instance, Lifecycle, Needs, Plan, PlannedFile, Program, TestId};

use crate::expand::{Expanded, Expander};
use crate::needs::Shared;
use crate::{PlanError, PlanOptions, Selection, select};

/// Plans a run of `selection` over `program`.
///
/// An empty selection result is a plan with no instances, not an error; the engine reports it as
/// "nothing ran".
///
/// # Errors
///
/// A [`PlanError`] when a selector cannot be read or matches nothing, a `:n` instance does not
/// exist or preloaded example rows are missing.
pub fn plan(program: &Program, selection: &Selection, options: &PlanOptions) -> Result<Plan, PlanError> {
    let picked = select::select(program, selection)?;
    let expanded = Expander::new(program, options, selection.include_skipped).expand(&picked)?;
    Ok(PlanBuilder::new(program).build(expanded))
}

/// The tests a run of `selection` plans whose rows come from a template that `options` does not
/// count yet, in plan order.
///
/// The engine renders those, and only those, then plans once (D132). A test the selection leaves
/// out, or skips, is not among them, nor is one whose rows come from a `$file`, which the compile
/// gives (unloaded, it is [`PlanError::RowsNotLoaded`]).
///
/// # Errors
///
/// A [`PlanError`] when a selector cannot be read or matches nothing.
pub fn rows_wanted(program: &Program, selection: &Selection, options: &PlanOptions) -> Result<Vec<TestId>, PlanError> {
    let picked = select::select(program, selection)?;
    Ok(Expander::new(program, options, selection.include_skipped).unloaded(&picked))
}

/// The rows a draw gave each test, in the order its `examples:` template gave them.
pub type DrawnRows<T> = HashMap<TestId, Vec<T>>;

/// Plans `selection` as a run whose templated `examples:` rows are drawn first: `draw` renders
/// the rows of the [`rows_wanted`] tests, all in one pass, and the plan counts them. A program
/// with no templated `examples` is planned at once, without calling `draw`. The rows come back
/// with the plan, for the run to bind each instance to its own.
///
/// The planner stays pure: drawing renders templates with the run's values, so it is the
/// caller's (the executor's `draw_rows`), passed in as `draw`.
///
/// # Errors
///
/// [`PlanError::Diagnostics`] holding the diagnostic `draw` fails with, else what [`plan`] and
/// [`rows_wanted`] fail with.
pub fn plan_drawn<T, D>(
    program: &Program,
    selection: &Selection,
    (mut options, draw): (PlanOptions, D),
) -> Result<(Plan, DrawnRows<T>), PlanError>
where
    D: FnOnce(&[TestId]) -> Result<DrawnRows<T>, Box<Diagnostic>>,
{
    let templated = program.tests.values().any(|test| matches!(test.examples, Some(Examples::Template(_))));
    let wanted = if templated { rows_wanted(program, selection, &options)? } else { Vec::new() };
    let mut rows = DrawnRows::new();
    if !wanted.is_empty() {
        rows = draw(&wanted).map_err(|diagnostic| PlanError::Diagnostics(vec![*diagnostic]))?;
        let counts = rows.iter().map(|(test, drawn)| (*test, u32::try_from(drawn.len()).unwrap_or(u32::MAX)));
        options.example_rows.extend(counts);
    }
    Ok((plan(program, selection, &options)?, rows))
}

/// Assembles a [`Plan`] from expanded instances: lifecycle blocks only where an instance runs
/// (D122), and every instance's needs.
struct PlanBuilder<'p> {
    program: &'p Program,
    run: Shared,
}

impl<'p> PlanBuilder<'p> {
    fn new(program: &'p Program) -> Self {
        Self { program, run: Shared::default() }
    }

    fn build(mut self, expanded: Expanded) -> Plan {
        let Expanded { files, skipped } = expanded;
        if files.is_empty() {
            return Plan { skipped, ..Plan::default() };
        }
        let blocks = &self.program.run_blocks;
        let before_run = self.run.plan(self.program, &blocks.before_run);
        let files = files.into_iter().map(|(id, instances)| self.file(id, instances)).collect();
        Plan { before_run, files, after_run: blocks.after_run.clone(), skipped }
    }

    fn file(&self, id: vc_model::FileId, mut instances: Vec<Instance>) -> PlannedFile {
        let none = Lifecycle::default();
        let lifecycle = self.program.test_files.get(&id).map_or(&none, |file| &file.lifecycle);
        let mut shared = self.run.clone();
        let before_all = shared.plan(self.program, &lifecycle.before_all);
        let mut needs: HashMap<TestId, Needs> = HashMap::new();
        for instance in &mut instances {
            let program = self.program;
            let test = needs.entry(instance.test).or_insert_with(|| {
                program.tests.get(&instance.test).map(|test| shared.instance(program, test)).unwrap_or_default()
            });
            instance.needs = test.clone();
        }
        PlannedFile { id, before_all, instances, after_all: lifecycle.after_all.clone() }
    }
}
