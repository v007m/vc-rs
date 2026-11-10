//! `$file` request bodies (D272). Each literal path is resolved from the folder of the file that
//! names it, so the program holds it from the project root, and [`body_files`] lists them for the
//! engine to read before a run. A templated path stays as written; the executor fails it with
//! V5111, since the bytes it names were not read.

use std::collections::BTreeSet;

use vc_model::{ActionKind, BodySpec, Part, Program, SourcePath, Template, TemplateValue};

/// The key of a single-key map that names a file.
const FILE: &str = "$file";

/// `body` with each literal `$file` path, the whole body's or a map's (multipart parts, D274),
/// resolved from `folder`. A path that leads out of the project stays as written (V2004).
pub fn resolved(body: BodySpec, folder: &[&str]) -> BodySpec {
    match body {
        BodySpec::File(template) => BodySpec::File(path(template, folder)),
        BodySpec::Json(value) => BodySpec::Json(value_resolved(value, folder)),
        other => other,
    }
}

/// The project paths of the literal `$file` bodies and parts of `program`'s apis and actions,
/// each once, in order: the engine reads them before a run (D272).
#[must_use]
pub fn body_files(program: &Program) -> Vec<SourcePath> {
    let apis = program.apis.values().map(|api| &api.body);
    let actions = program.actions.iter().flat_map(|action| {
        let inline = match &action.kind {
            ActionKind::Inline(inline) => Some(&inline.body),
            _ => None,
        };
        action.body.as_ref().into_iter().chain(inline)
    });
    let mut paths = BTreeSet::new();
    apis.chain(actions).for_each(|body| named(body, &mut paths));
    paths.into_iter().filter_map(|path| SourcePath::new(path).ok()).collect()
}

fn value_resolved(value: TemplateValue, folder: &[&str]) -> TemplateValue {
    match value {
        TemplateValue::Map(map) => {
            let single = map.len() == 1;
            TemplateValue::Map(
                map.into_iter().map(|(key, value)| entry_resolved(key, value, (single, folder))).collect(),
            )
        }
        TemplateValue::List(items) => {
            TemplateValue::List(items.into_iter().map(|item| value_resolved(item, folder)).collect())
        }
        other => other,
    }
}

fn entry_resolved(key: String, value: TemplateValue, (single, folder): (bool, &[&str])) -> (String, TemplateValue) {
    let value = match value {
        TemplateValue::String(template) if single && key == FILE => TemplateValue::String(path(template, folder)),
        value => value_resolved(value, folder),
    };
    (key, value)
}

/// `template` resolved from `folder` when it is a literal path inside the project.
fn path(template: Template, folder: &[&str]) -> Template {
    let resolved = template.as_literal().and_then(|written| vc_model::resolve_path(folder, written));
    resolved.map_or(template, |path| Template { parts: vec![Part::Literal(path)] })
}

fn named<'p>(body: &'p BodySpec, paths: &mut BTreeSet<&'p str>) {
    match body {
        BodySpec::File(template) => paths.extend(template.as_literal()),
        BodySpec::Json(value) => named_in(value, paths),
        _ => {}
    }
}

fn named_in<'p>(value: &'p TemplateValue, paths: &mut BTreeSet<&'p str>) {
    match value {
        TemplateValue::Map(map) => match map.get(FILE) {
            Some(TemplateValue::String(template)) if map.len() == 1 => paths.extend(template.as_literal()),
            _ => map.values().for_each(|value| named_in(value, paths)),
        },
        TemplateValue::List(items) => items.iter().for_each(|item| named_in(item, paths)),
        _ => {}
    }
}

#[cfg(test)]
#[path = "body_files_tests.rs"]
mod tests;
