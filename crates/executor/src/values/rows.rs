//! A test's example rows: inline rows render per instance; the rows an `examples: ${…}` template
//! gives render once before the run, reading only `var`, `env` and `dataset` (V2105).

use serde_json::Value;
use vc_interfaces::Random;
use vc_model::{Examples, Program, Test};
use vc_template::RenderError;

use super::render::{Render, Shared};
use super::run::RunValues;
use super::scope::Scope;
use crate::{Redactor, Schemas};

/// The rows `test`'s `examples:` template gives, or `None` when its rows are written in the
/// test or come from a file.
///
/// `shuffle` and `random` draw from `random`, the test's own source, so its rows do not depend on
/// what else the run renders.
///
/// # Errors
///
/// When the template does not render, or gives anything but a list of maps.
pub fn example_rows(
    program: &Program,
    (values, random): (&RunValues, &dyn Random),
    test: &Test,
) -> Result<Option<Vec<Value>>, RenderError> {
    let Some(Examples::Template(template)) = &test.examples else { return Ok(None) };
    let (redactor, schemas) = (Redactor::new(&[]), Schemas::default());
    let shared = Shared { values, program, redactor: &redactor, schemas: &schemas };
    match Render::over(shared, Scope::of_test(values, test), random).template(template)? {
        Value::Array(rows) if rows.iter().all(Value::is_object) => Ok(Some(rows)),
        _ => Err(RenderError::invalid("`examples` must give a list of maps, one per example row")),
    }
}

impl Render<'_> {
    /// Row `at` of `test`'s examples: an inline row rendered now, or a row its template gave
    /// before the run, as it is, so the data in it is never read as a template.
    pub(super) fn example(&self, test: &Test, at: usize) -> Result<Value, RenderError> {
        let absent = || RenderError::invalid("the example row is not in the test");
        match &test.examples {
            Some(Examples::Rows(rows) | Examples::Data(rows)) => self.value(rows.get(at).ok_or_else(absent)?),
            Some(Examples::Template(_)) => {
                let rows = self.values().inputs().examples.get(&test.id);
                rows.and_then(|rows| rows.get(at)).cloned().ok_or_else(absent)
            }
            Some(Examples::File(_)) | None => {
                Err(RenderError::invalid("examples from a file arrive in a later milestone"))
            }
        }
    }
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
