//! The rows of each test whose `examples:` is a template, rendered once while the run is planned,
//! and only for the tests it selects and does not skip: the planner counts them and each instance
//! binds its row as it is (syntax reference "Examples"). A test whose examples gave no rows is
//! skipped with a notice.

use std::iter;
use std::sync::Arc;

use vc_diagnostics::{Code, Diagnostic};
use vc_executor::{ExampleRows, RunInputs, RunValues};
use vc_interfaces::Random;
use vc_model::{Examples, Plan, Program, Test, TestId};
use vc_planner::{NO_ROWS, PlanError, PlanOptions, Selection};
use vc_template::RenderError;

use crate::EngineError;
use crate::plan::plan_error;
use crate::ports;
use crate::project::invalid;

/// What the help of a templated `examples:` that does not render says.
const HELP: &str = "`examples` is read before the run starts: it reads only `var`, `env` and `dataset`, and must give a \
                    list of maps, one per example row";

/// The run's random source, its seed for `--seed` to repeat it, and the example rows drawn from it.
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

/// Plans `selection`, rendering a templated test's rows only when the planner first asks for
/// them, so a test the run does not select, or skips, is never rendered.
pub fn planned(loader: &mut Loader<'_>, selection: &Selection, mut options: PlanOptions) -> Result<Plan, EngineError> {
    loop {
        match vc_planner::plan(loader.program, selection, &options) {
            Ok(plan) => return Ok(plan),
            Err(error) => loader.load(error, &mut options)?,
        }
    }
}

/// A notice for each planned test skipped because its examples gave no rows, so a run that
/// passes without it says so.
pub fn empty(program: &Program, plan: &Plan) -> Vec<String> {
    let empty = plan.skipped.iter().filter(|skipped| skipped.reason == NO_ROWS);
    empty.map(|skipped| format!("`{}`: {NO_ROWS}, so it is skipped", name(program, skipped.test))).collect()
}

/// Renders the rows of the templated tests the planner asks for, with the run's profiles, vars,
/// environment and random source, into the run's draw.
pub struct Loader<'a> {
    program: &'a Program,
    inputs: RunInputs,
    values: Option<RunValues>,
    draw: Draw,
}

impl<'a> Loader<'a> {
    /// A loader over `program` that renders with `inputs` into `draw`; the values are built when
    /// the first rows are asked for.
    pub const fn new(program: &'a Program, inputs: RunInputs, draw: Draw) -> Self {
        Self { program, inputs, values: None, draw }
    }

    /// The draw, with the rows rendered so far.
    pub fn into_draw(self) -> Draw {
        self.draw
    }

    /// Renders the rows of the test the planner's `error` asks for, and counts them in
    /// `options`; the refusal when it cannot, or when the planner refused for another reason.
    fn load(&mut self, error: PlanError, options: &mut PlanOptions) -> Result<(), EngineError> {
        let program = self.program;
        let asked = match &error {
            PlanError::RowsNotLoaded { test } => templated(program, test),
            _ => None,
        };
        let Some(test) = asked.filter(|test| !options.example_rows.contains_key(&test.id)) else {
            return Err(plan_error(error, program));
        };
        let inputs = &self.inputs;
        let values = self.values.get_or_insert_with(|| RunValues::new(program, inputs.clone()));
        match vc_executor::example_rows(program, values, test) {
            Ok(Some(rows)) => {
                let count = u32::try_from(rows.len()).unwrap_or(u32::MAX);
                options.example_rows.insert(test.id, count);
                drop(self.draw.rows.insert(test.id, rows));
                Ok(())
            }
            Ok(None) => Err(plan_error(error, program)),
            Err(failed) => Err(refused(program, test, &failed)),
        }
    }
}

/// The test whose full ID is `id`, when its `examples:` is a template.
fn templated<'a>(program: &'a Program, id: &str) -> Option<&'a Test> {
    let named = |test: &&Test| program.str(test.id.sym()) == Some(id);
    program.tests.values().find(named).filter(|test| matches!(test.examples, Some(Examples::Template(_))))
}

/// The check report for a test whose rows did not render.
fn refused(program: &Program, test: &Test, error: &RenderError) -> EngineError {
    invalid(iter::once(diagnostic(program, test, error)).collect(), program.files.clone(), Vec::new())
}

/// V5111 (or the code `error` names) at `test`: its examples did not render.
fn diagnostic(program: &Program, test: &Test, error: &RenderError) -> Diagnostic {
    let message = format!("the examples of `{}` did not render: {}", name(program, test.id), error.message);
    Diagnostic::new(error.code.unwrap_or(Code::V5111), test.span, message).help(HELP)
}

fn name(program: &Program, test: TestId) -> String {
    program.str(test.sym()).unwrap_or_default().to_owned()
}
