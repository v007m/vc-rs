//! The label of each planned example row: its `name`, when it gives one as plain text. The
//! engine and the playground both label rows here, so a report names a row the same in each.

use std::collections::BTreeMap;

use vc_model::{Examples, Plan, Program, TemplateValue, TestId};

use super::run::ExampleRows;

/// The `name` of each planned example row that gives one as plain text, in a row written inline
/// or in a row an `examples:` template gave (`rendered`, as [`super::draw_rows`] drew them): the
/// instance's label.
#[must_use]
pub fn row_names(program: &Program, plan: &Plan, rendered: &ExampleRows) -> BTreeMap<(TestId, u32), String> {
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

#[cfg(test)]
#[path = "labels_tests.rs"]
mod tests;
