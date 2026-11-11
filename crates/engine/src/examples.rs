//! The rows of each test whose `examples:` is a template, rendered once while the run is planned,
//! and only for the tests it selects and does not skip: the planner counts them and each instance
//! binds its row as it is (syntax reference "Examples"). Each test's rows draw from a random
//! source of their own, from the run's seed and the test's full ID, so a seed gives a test the
//! same rows whatever else the run selects. Rows from a `$file` are the compile's to give, never
//! rendered here. A test whose examples gave no rows is skipped with a notice.

use std::collections::BTreeMap;
use std::iter;
use std::sync::Arc;

use serde_json::Value;
use vc_diagnostics::{Code, Diagnostic};
use vc_executor::{ExampleRows, Redactor, RunInputs, RunValues, UnreadVar};
use vc_interfaces::Random;
use vc_model::{Examples, Plan, Program, TemplateValue, Test, TestId};
use vc_planner::{NO_ROWS, PlanOptions, Selection};
use vc_template::{PipeRegistry, ROW_HELP, RenderError};

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
/// the run does not skip, all in one pass and into `draw`, then plans once with their counts. A
/// program with no templated `examples` is planned at once.
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
        for test in wanted.iter().filter_map(|id| program.tests.get(id)) {
            let Some(rows) = rendered(program, (&values, draw.seed), test)? else { continue };
            options.example_rows.insert(test.id, u32::try_from(rows.len()).unwrap_or(u32::MAX));
            drop(draw.rows.insert(test.id, rows));
        }
    }
    let mut plan = vc_planner::plan(program, selection, &options).map_err(|error| plan_error(error, program))?;
    plan.row_names = row_names(program, &plan, &draw.rows);
    Ok(plan)
}

/// The `name` of each planned example row that gives one as plain text, in a row written inline
/// or in a row an `examples:` template gave: the instance's label.
fn row_names(program: &Program, plan: &Plan, rendered: &ExampleRows) -> BTreeMap<(TestId, u32), String> {
    let instances = plan.files.iter().flat_map(|file| &file.instances);
    let rows = instances.filter_map(|instance| Some((instance.test, instance.example_row?)));
    rows.filter_map(|slot| Some((slot, row_name(program, rendered, slot)?))).collect()
}

/// Row `row`'s `name` of `test`, when it is non-empty text; a `name` with a reference in an
/// inline row is rendered only as the instance runs, so it does not label it.
fn row_name(program: &Program, rendered: &ExampleRows, (test, row): (TestId, u32)) -> Option<String> {
    let at = usize::try_from(row).ok()?;
    let name = match &program.tests.get(&test)?.examples {
        Some(Examples::Rows(rows)) => {
            let Some(TemplateValue::Map(row)) = rows.get(at) else { return None };
            let Some(TemplateValue::String(name)) = row.get("name") else { return None };
            name.as_literal()?
        }
        Some(Examples::Template(_)) => rendered.get(&test)?.get(at)?.get("name")?.as_str()?,
        Some(Examples::File(_)) | None => return None,
    };
    (!name.is_empty()).then(|| name.to_owned())
}

/// Masks each example-row label in `plan` with `redactor`, as the run's events are masked: a row
/// `name` a template drew from an env var or a dataset holding a secret's value reads `••••` in
/// every report, the history and the run's JSON.
pub async fn mask_labels(plan: &mut Plan, redactor: &Redactor) {
    for name in plan.row_names.values_mut() {
        *name = redactor.masked_text(name).await;
    }
}

/// Whether running `plan` generates data from the run's seed: a `${fake.…}`, or a built-in pipe
/// that draws from it, in anything the run reads. Only then does `--seed` repeat something.
pub fn generates(program: &Program, plan: &Plan) -> bool {
    let pipes = PipeRegistry::builtin();
    vc_planner::generates(program, plan, |name| pipes.get(name).is_some_and(|pipe| pipe.draws))
}

/// A notice for each planned test skipped because its examples gave no rows, so a run that
/// passes without it says so.
pub fn empty(program: &Program, plan: &Plan) -> Vec<String> {
    let empty = plan.skipped.iter().filter(|skipped| skipped.reason == NO_ROWS);
    empty.map(|skipped| format!("`{}`: {NO_ROWS}, so it is skipped", name(program, skipped.test))).collect()
}

/// The rows of `test`, a templated test, from its own random source; V2105 when a var they read
/// needs a root with no value before the run under the run's profiles, and the render's error
/// when they do not render; `None` for rows that are not a template, which the planner then
/// reports as not loaded.
fn rendered(
    program: &Program,
    (values, seed): (&RunValues, u64),
    test: &Test,
) -> Result<Option<Vec<Value>>, EngineError> {
    if let Some(unread) = vc_executor::unread_var(program, values, test) {
        return Err(refused(program, unread_diagnostic(&unread)));
    }
    let random = ports::random_for(seed, &name(program, test.id));
    let rows = vc_executor::example_rows(program, (values, random.as_ref()), test);
    rows.map_err(|failed| refused(program, unrendered(program, test, &failed)))
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

#[cfg(test)]
#[path = "examples_tests.rs"]
mod tests;
