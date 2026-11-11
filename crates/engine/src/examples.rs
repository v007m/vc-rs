//! The rows of each test whose `examples:` is a template, rendered once while the run is planned,
//! and only for the tests it selects and does not skip: the planner counts them and each instance
//! binds its row as it is (syntax reference "Examples"). Each test's rows draw from a random
//! source of their own, from the run's seed and the test's full ID, so a seed gives a test the
//! same rows whatever else the run selects. A test whose examples gave no rows is skipped with a
//! notice.

use std::iter;
use std::sync::Arc;

use serde_json::Value;
use vc_diagnostics::{Code, Diagnostic};
use vc_executor::{ExampleRows, RunInputs, RunValues, UnreadVar};
use vc_interfaces::Random;
use vc_model::{Plan, Program, Test, TestId};
use vc_planner::{NO_ROWS, PlanOptions, Selection};
use vc_template::{ROW_HELP, RenderError};

use crate::EngineError;
use crate::plan::plan_error;
use crate::ports;
use crate::project::invalid;

/// What the help of a templated `examples:` that gives no rows before the run says.
const LIST_HELP: &str = "it must give a list of maps, one per example row";

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
/// the run does not skip, all in one pass and into `draw`, then plans once with their counts.
pub fn planned(
    program: &Program,
    selection: &Selection,
    (inputs, draw, mut options): (RunInputs, &mut Draw, PlanOptions),
) -> Result<Plan, EngineError> {
    options.skip_empty = true;
    let wanted = vc_planner::rows_wanted(program, selection, &options).map_err(|error| plan_error(error, program))?;
    if !wanted.is_empty() {
        let values = RunValues::new(program, inputs);
        for test in wanted.iter().filter_map(|id| program.tests.get(id)) {
            let rows = rendered(program, (&values, draw.seed), test)?;
            options.example_rows.insert(test.id, u32::try_from(rows.len()).unwrap_or(u32::MAX));
            drop(draw.rows.insert(test.id, rows));
        }
    }
    vc_planner::plan(program, selection, &options).map_err(|error| plan_error(error, program))
}

/// A notice for each planned test skipped because its examples gave no rows, so a run that
/// passes without it says so.
pub fn empty(program: &Program, plan: &Plan) -> Vec<String> {
    let empty = plan.skipped.iter().filter(|skipped| skipped.reason == NO_ROWS);
    empty.map(|skipped| format!("`{}`: {NO_ROWS}, so it is skipped", name(program, skipped.test))).collect()
}

/// `test`'s rows, from its own random source; V2105 when a var they read needs a root with no
/// value before the run under the run's profiles, and the render's error when they do not render.
fn rendered(program: &Program, (values, seed): (&RunValues, u64), test: &Test) -> Result<Vec<Value>, EngineError> {
    if let Some(unread) = vc_executor::unread_var(program, values, test) {
        return Err(refused(program, unread_diagnostic(&unread)));
    }
    let random = ports::random_for(seed, &name(program, test.id));
    match vc_executor::example_rows(program, (values, random.as_ref()), test) {
        Ok(Some(rows)) => Ok(rows),
        Ok(None) => Err(refused(
            program,
            unrendered(program, test, &RenderError::invalid("their `$file` was not read before the run")),
        )),
        Err(failed) => Err(refused(program, unrendered(program, test, &failed))),
    }
}

/// The check report holding `diagnostic`.
fn refused(program: &Program, diagnostic: Diagnostic) -> EngineError {
    invalid(iter::once(diagnostic).collect(), program.files.clone(), Vec::new())
}

/// V2105 at the `${var.…}` in `examples` whose var needs a root with no value before the run.
fn unread_diagnostic(unread: &UnreadVar) -> Diagnostic {
    let needs = vc_template::row_root_label(unread.root);
    let (message, help) = vc_template::row_var_unread(&unread.read, &unread.var, &needs);
    Diagnostic::new(Code::V2105, unread.span, message).help(help)
}

/// V5111 (or the code `error` names) at `test`: its examples did not render.
fn unrendered(program: &Program, test: &Test, error: &RenderError) -> Diagnostic {
    let message = format!("the examples of `{}` did not render: {}", name(program, test.id), error.message);
    Diagnostic::new(error.code.unwrap_or(Code::V5111), test.span, message).help(format!("{ROW_HELP}; {LIST_HELP}"))
}

fn name(program: &Program, test: TestId) -> String {
    program.str(test.sym()).unwrap_or_default().to_owned()
}
