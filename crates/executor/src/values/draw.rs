//! The rows of each templated test a run selects, drawn once before it is planned: the engine
//! and the playground both draw them here, so the same seed gives the same rows in each.

use std::sync::Arc;

use serde_json::Value;
use vc_diagnostics::{Code, Diagnostic};
use vc_interfaces::Random;
use vc_model::{Program, Test, TestId};
use vc_template::{ROW_HELP, RenderError};

use super::rows::example_rows;
use super::run::{ExampleRows, RunValues};
use super::unread::{UnreadVar, unread_var};

/// What the help of a templated `examples:` that gives no rows before the run says.
const LIST_HELP: &str = "it must give a list of maps, one per example row";

/// A test's own random source, from its full ID: `users.crud#listsPages`.
pub type RandomFor<'a> = &'a dyn Fn(&str) -> Arc<dyn Random>;

/// The seed of one test's example rows, from the run's `seed` and the test's full `id`: the
/// rows a seed gives a test do not depend on what else the run renders, or in what order.
#[must_use]
pub fn row_seed(seed: u64, id: &str) -> u64 {
    seed ^ fnv1a(id)
}

/// The 64-bit FNV-1a hash of `text`, the same on every build and platform (unlike `DefaultHasher`).
fn fnv1a(text: &str) -> u64 {
    let step = |hash: u64, byte: u8| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    text.bytes().fold(0xcbf2_9ce4_8422_2325, step)
}

/// The rows of each of `wanted` whose `examples:` is a template.
///
/// Each test's rows come from `random_for(its full ID)`. Tests whose rows are written in the
/// test or come from a file are left out, for the planner to count or report.
///
/// # Errors
///
/// V2105 when a var the rows read needs a root with no value before the run, under the run's
/// profiles; V5111 (or the render's own code) when they do not render.
pub fn draw_rows(
    program: &Program,
    values: &RunValues,
    (wanted, random_for): (&[TestId], RandomFor<'_>),
) -> Result<ExampleRows, Box<Diagnostic>> {
    let mut drawn = ExampleRows::new();
    for test in wanted.iter().filter_map(|id| program.tests.get(id)) {
        if let Some(rows) = rendered(program, (values, random_for), test)? {
            drop(drawn.insert(test.id, rows));
        }
    }
    Ok(drawn)
}

/// The rows of `test`, from its own random source; `None` when they are not a template.
fn rendered(
    program: &Program,
    (values, random_for): (&RunValues, RandomFor<'_>),
    test: &Test,
) -> Result<Option<Vec<Value>>, Box<Diagnostic>> {
    if let Some(unread) = unread_var(program, values, test) {
        return Err(Box::new(unread_diagnostic(&unread)));
    }
    let random = random_for(&name(program, test.id));
    example_rows(program, (values, random.as_ref()), test)
        .map_err(|failed| Box::new(unrendered(program, test, &failed)))
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
#[path = "draw_tests.rs"]
mod tests;
