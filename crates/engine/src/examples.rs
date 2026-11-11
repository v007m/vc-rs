//! The rows of each test whose `examples:` is a template, rendered once while the run is planned,
//! and only for the tests it selects and does not skip: the planner counts them and each instance
//! binds its row as it is (syntax reference "Examples"). Each test's rows draw from a random
//! source of their own, from the run's seed and the test's full ID, so a seed gives a test the
//! same rows whatever else the run selects. Rows from a `$file` are the compile's to give, never
//! rendered here. A test whose examples gave no rows is skipped with a notice
//! ([`vc_planner::no_rows_notices`]).

use std::sync::Arc;

use vc_executor::{ExampleRows, RunInputs, RunValues};
use vc_interfaces::Random;
use vc_model::{Plan, Program, TestId};
use vc_planner::{PlanOptions, Selection};

use crate::EngineError;
use crate::plan::plan_error;
use crate::ports;

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

/// Plans `selection` as a run ([`vc_planner::plan_drawn`]): renders, with `inputs`, the rows of
/// the selected templated tests the run does not skip, all in one pass and into `draw`
/// ([`vc_executor::draw_rows`]), then plans once with their counts.
pub fn planned(
    program: &Program,
    selection: &Selection,
    (inputs, draw, options): (RunInputs, &mut Draw, PlanOptions),
) -> Result<Plan, EngineError> {
    let seed = draw.seed;
    let rows = |wanted: &[TestId]| {
        let values = RunValues::new(program, inputs);
        let random_for = |name: &str| ports::random_for(seed, name);
        vc_executor::draw_rows(program, &values, (wanted, &random_for))
    };
    let (plan, rows) =
        vc_planner::plan_drawn(program, selection, (options, rows)).map_err(|error| plan_error(error, program))?;
    draw.rows = rows;
    Ok(plan)
}

#[cfg(test)]
#[path = "examples_tests.rs"]
mod tests;
