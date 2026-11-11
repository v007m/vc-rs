//! The rows of each test whose `examples:` is a template, rendered once while the run is planned,
//! and only for the tests it selects and does not skip: the planner counts them and each instance
//! binds its row as it is (syntax reference "Examples"). Each test's rows draw from a random
//! source of their own, from the run's seed and the test's full ID, so a seed gives a test the
//! same rows whatever else the run selects. Rows from a `$file` are the compile's to give, never
//! rendered here. A test whose examples gave no rows is skipped with a notice
//! ([`vc_planner::no_rows_notices`]).

use std::iter;
use std::sync::Arc;

use vc_diagnostics::Diagnostic;
use vc_executor::{ExampleRows, RunInputs, RunValues};
use vc_interfaces::Random;
use vc_model::{Examples, Plan, Program, TestId};
use vc_planner::{PlanOptions, Selection};

use crate::EngineError;
use crate::plan::plan_error;
use crate::ports;
use crate::project::invalid;

/// The run's random source, its seed for `--seed` to repeat it, and the example rows drawn for it.
pub struct Draw {
    pub seed: u64,
    pub random: Arc<dyn Random>,
    pub rows: ExampleRows,
}

impl Draw {
    /// A draw from `seed`, or a fresh seed, with no rows yet.
    pub fn new(seed: Option<u64>) -> Self {
        let (seed, random) = ports::random(seed);
        Self { seed, random, rows: ExampleRows::new() }
    }
}

/// Plans `selection` as a run: renders, with `inputs`, the rows of the selected templated tests
/// the run does not skip, all in one pass and into `draw` ([`vc_executor::draw_rows`]), then
/// plans once with their counts. A program with no templated `examples` is planned at once.
pub fn planned(
    program: &Program,
    selection: &Selection,
    (inputs, draw, mut options): (RunInputs, &mut Draw, PlanOptions),
) -> Result<Plan, EngineError> {
    let templated = program.tests.values().any(|test| matches!(test.examples, Some(Examples::Template(_))));
    let wanted = if templated { vc_planner::rows_wanted(program, selection, &options) } else { Ok(Vec::new()) };
    let wanted = wanted.map_err(|error| plan_error(error, program))?;
    if !wanted.is_empty() {
        let values = RunValues::new(program, inputs);
        let seed = draw.seed;
        let random_for = |name: &str| ports::random_for(seed, name);
        let rows = vc_executor::draw_rows(program, &values, (&wanted, &random_for));
        draw.rows = rows.map_err(|diagnostic| refused(program, *diagnostic))?;
        options.example_rows.extend(row_counts(&draw.rows));
    }
    vc_planner::plan(program, selection, &options).map_err(|error| plan_error(error, program))
}

/// How many rows each test drew, for the planner.
pub fn row_counts(rows: &ExampleRows) -> impl Iterator<Item = (TestId, u32)> + '_ {
    rows.iter().map(|(test, rows)| (*test, u32::try_from(rows.len()).unwrap_or(u32::MAX)))
}

/// The check report holding `diagnostic`.
fn refused(program: &Program, diagnostic: Diagnostic) -> EngineError {
    invalid(iter::once(diagnostic).collect(), program.files.clone(), Vec::new())
}

#[cfg(test)]
#[path = "examples_tests.rs"]
mod tests;
