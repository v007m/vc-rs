//! Shared blocks and profiles: a `beforeRun` or `beforeAll` action that saves values needs an
//! `id` to share them under (V2210), and a project needs a profile (V3101).

use std::collections::HashSet;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{FileKind, SourceSpan};
use vc_parser::Node;

use super::ids;
use super::project;
use super::project::{Project, Source};
use super::saves::Saves;

/// V2210 for every shared action without an `id` that saves something.
pub fn check(project: &Project<'_>, diagnostics: &mut Diagnostics) {
    let mut saves = Saves::new(project);
    for source in &project.sources {
        let key = match source.kind {
            FileKind::Config => "beforeRun",
            FileKind::Test => "beforeAll",
            FileKind::Api | FileKind::Load => continue,
        };
        let taken: HashSet<&str> = ids::declared(source).iter().filter_map(|node| node.as_str()).collect();
        for action in project::list(source.root, key).iter().filter(|action| action.get("id").is_none()) {
            let names = saves.action(source, action);
            if !names.is_empty() {
                diagnostics.push(missing_id(source, (action, key), &taken));
            }
        }
    }
}

fn missing_id(source: &Source<'_>, (action, block): (&Node, &str), taken: &HashSet<&str>) -> Diagnostic {
    let span = action.entries().first().map_or(action.span, |entry| entry.key_span);
    let message = format!("this {block} action saves values but has no `id`");
    let diagnostic = Diagnostic::new(Code::V2210, span, message).label("needs an `id`");
    let diagnostic = diagnostic.help("shared values are read as `${var.<id>.<name>}`; give the action an `id`");
    let Some(id) = derived(action) else { return diagnostic };
    let start = span.start;
    let indent = " ".repeat(usize::try_from(start.column).unwrap_or(1).saturating_sub(1));
    let edit = TextEdit::replace(SourceSpan::new(source.file, start, start), format!("id: {id}\n{indent}"));
    let title = format!("Add id: {id}");
    let fix = if taken.contains(id) { Fix::suggested(title, vec![edit]) } else { Fix::certain(title, vec![edit]) };
    diagnostic.fix(fix)
}

/// The ID an action would take: the short ID of the api it calls or the test it reuses.
fn derived(action: &Node) -> Option<&str> {
    let written = project::text(action, "call").or_else(|| project::text(action, "test"))?;
    Some(written.rsplit('#').next().unwrap_or(written))
}

/// V3101 when the config defines no profile.
pub fn profiles(project: &Project<'_>) -> Option<Diagnostic> {
    let config = project.config().filter(|config| !config.broken)?;
    if !project::list(config.root, "profiles").is_empty() {
        return None;
    }
    let root = config.root;
    let first = root.entries().first().map(|entry| entry.key_span);
    let span = root.entry("syntaxVersion").map(|entry| entry.key_span).or(first).unwrap_or(root.span);
    let diagnostic = Diagnostic::new(Code::V3101, span, "the config defines no profile").label("in this config");
    Some(diagnostic.help("add `profiles:` with one profile marked `default: true`, such as `- id: dev`"))
}
