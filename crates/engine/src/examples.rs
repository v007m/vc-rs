//! The rows of each test whose `examples:` is a template, rendered once before the run is planned:
//! the planner counts them and each instance binds its row as it is (syntax reference "Examples").

use std::collections::BTreeMap;
use std::sync::Arc;

use vc_diagnostics::{Code, Diagnostic};
use vc_executor::{ExampleRows, RunInputs, RunValues};
use vc_interfaces::Random;
use vc_model::{Examples, Program, Test, TestId};
use vc_planner::PlanError;
use vc_template::RenderError;

use crate::EngineError;
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

/// What rendering the templated examples left: the reason each test whose rows did not render
/// failed, by the test's full ID.
#[derive(Default)]
pub struct Failed(BTreeMap<String, Diagnostic>);

impl Failed {
    /// The check report for a test the planner could not expand because its rows did not render.
    pub fn refused(&self, error: &PlanError, program: &Program) -> Option<EngineError> {
        let PlanError::RowsNotLoaded { test } = error else { return None };
        let found = self.0.get(test)?.clone();
        Some(invalid(std::iter::once(found).collect(), program.files.clone(), Vec::new()))
    }
}

/// Renders the rows of every test in `program` whose `examples:` is a template, with `inputs`'
/// profiles, vars, environment and random source, into `draw`.
pub fn render(program: &Program, inputs: RunInputs, draw: &mut Draw) -> Failed {
    let mut failed = Failed::default();
    let templated: Vec<&Test> =
        program.tests.values().filter(|test| matches!(test.examples, Some(Examples::Template(_)))).collect();
    if templated.is_empty() {
        return failed;
    }
    let values = RunValues::new(program, inputs);
    for test in templated {
        match vc_executor::example_rows(program, &values, test) {
            Ok(Some(rows)) => drop(draw.rows.insert(test.id, rows)),
            Ok(None) => {}
            Err(error) => drop(failed.0.insert(name(program, test.id), diagnostic(program, test, &error))),
        }
    }
    failed
}

/// How many rows each templated test has, for the planner.
pub fn counts(rows: &ExampleRows) -> BTreeMap<TestId, u32> {
    rows.iter().map(|(id, rows)| (*id, u32::try_from(rows.len()).unwrap_or(u32::MAX))).collect()
}

/// V5111 (or the code `error` names) at `test`: its examples did not render.
fn diagnostic(program: &Program, test: &Test, error: &RenderError) -> Diagnostic {
    let message = format!("the examples of `{}` did not render: {}", name(program, test.id), error.message);
    Diagnostic::new(error.code.unwrap_or(Code::V5111), test.span, message).help(HELP)
}

fn name(program: &Program, test: TestId) -> String {
    program.str(test.sym()).unwrap_or_default().to_owned()
}
