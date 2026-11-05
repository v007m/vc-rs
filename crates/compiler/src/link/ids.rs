//! The ID tables: duplicates (V2001), reserved IDs (V2006) and file IDs (V2007).

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{FileKind, SourceSpan};
use vc_parser::Node;

use super::project;
use super::project::{Project, Source};

/// The root names no `id` may take (D128, D137).
const RESERVED: [&str; 11] =
    ["var", "env", "secret", "response", "request", "example", "item", "iteration", "fake", "dataset", "run"];

/// The config lists whose items have IDs; `datasets.files` is the other one.
const CONFIG_ITEMS: [&str; 4] = ["profiles", "auth", "generators", "plugins"];

/// Every `id` value node a file declares, in file order.
pub fn declared<'a>(source: &Source<'a>) -> Vec<&'a Node> {
    let items: &[&str] = match source.kind {
        FileKind::Api => &["apis"],
        FileKind::Test => &["tests"],
        FileKind::Config => &CONFIG_ITEMS,
        FileKind::Load => &[],
    };
    let datasets = source.root.get("datasets").filter(|_| source.kind == FileKind::Config);
    let datasets = datasets.map(|datasets| project::list(datasets, "files")).unwrap_or_default();
    let nodes = items.iter().flat_map(|key| project::list(source.root, key)).chain(datasets);
    let nodes = nodes.chain(project::actions(source));
    let mut ids: Vec<&Node> = nodes.filter_map(|node| node.get("id")).collect();
    ids.sort_by_key(|node| node.span.start.offset);
    ids
}

/// Checks every file's IDs and the file IDs.
pub fn check(project: &Project<'_>, diagnostics: &mut Diagnostics) {
    for source in &project.sources {
        within_file(source, diagnostics);
        diagnostics.extend(explicit_id(source));
    }
    let mut seen: HashMap<&str, SourceSpan> = HashMap::new();
    for source in &project.sources {
        let Some(id) = source.id.as_deref() else { continue };
        let spot = file_spot(source);
        if let Some(first) = seen.get(id) {
            let message = format!("two files have the ID `{id}`");
            let diagnostic = Diagnostic::new(Code::V2001, spot, message).label("this file's ID");
            let diagnostic = diagnostic.secondary(*first, "the other file with that ID");
            diagnostics.push(diagnostic.help("rename or move one of the files"));
        } else {
            seen.insert(id, spot);
        }
    }
}

/// V2001 for an ID declared twice in one file and V2006 for a reserved one.
fn within_file(source: &Source<'_>, diagnostics: &mut Diagnostics) {
    let mut seen: HashMap<&str, SourceSpan> = HashMap::new();
    for node in declared(source) {
        let Some(id) = node.as_scalar().map(|scalar| scalar.text.as_str()) else { continue };
        if RESERVED.contains(&id) {
            let message = format!("`{id}` is a reserved name and cannot be an ID");
            let diagnostic = Diagnostic::new(Code::V2006, node.span, message).label("reserved");
            diagnostics.push(diagnostic.help(format!("pick another ID; `{id}` is a template root")));
        }
        match seen.get(id) {
            Some(first) => diagnostics.push(duplicate(id, node.span, *first)),
            None => {
                seen.insert(id, node.span);
            }
        }
    }
}

fn duplicate(id: &str, span: SourceSpan, first: SourceSpan) -> Diagnostic {
    let diagnostic = Diagnostic::new(Code::V2001, span, format!("duplicate ID `{id}`")).label("declared again");
    let diagnostic = diagnostic.secondary(first, "first declared here");
    diagnostic.help("IDs are unique within a file across every kind; rename one")
}

/// V2007 for an `id:` that is not the ID the path gives.
fn explicit_id(source: &Source<'_>) -> Option<Diagnostic> {
    let derived = source.id.as_deref()?;
    let node = source.root.get("id")?;
    let written = node.as_scalar()?.text.as_str();
    if written == derived {
        return None;
    }
    let message = format!("the file ID is `{derived}` from its path, not `{written}`");
    let diagnostic = Diagnostic::new(Code::V2007, node.span, message).label("does not match the path");
    let diagnostic = diagnostic.help("remove the `id:` key, or move the file to match it");
    if node.span.byte_range().len() != written.len() {
        return Some(diagnostic);
    }
    let edit = TextEdit::replace(node.span, derived);
    Some(diagnostic.fix(Fix::certain(format!("Write {derived}"), vec![edit])))
}

/// Where a whole-file problem is shown: its `id:`, else its first key, else its start.
fn file_spot(source: &Source<'_>) -> SourceSpan {
    let first = source.root.entries().first().map(|entry| entry.key_span);
    source.root.get("id").map(|node| node.span).or(first).unwrap_or(source.root.span)
}
