//! `source.openapi` paths (V2004): the spec file an api file syncs from must be in the project.
//! The path is relative to the project root; a URL, or a path holding a template, is not checked.

use std::collections::BTreeSet;

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FileKind, SourceFiles, SourcePath};
use vc_parser::Node;

use super::assets::{self, Known};
use super::project::{Project, Source};

/// Checks the `source.openapi` path of every api file against the definition files and `assets`.
pub fn check(project: &Project<'_>, files: &SourceFiles, assets: &BTreeSet<SourcePath>) -> Diagnostics {
    let known = Known { paths: assets::paths(files, assets), folder: Vec::new(), what: "`source.openapi` paths" };
    let apis = project.sources.iter().filter(|source| source.kind == FileKind::Api);
    let specs = apis.filter_map(|source| Some((source, source.root.get("source")?.get("openapi")?)));
    specs.filter_map(|(source, node)| missing(source, node, &known)).collect()
}

/// V2004 when the spec at `node`, a path rather than a URL, names no file in the project.
fn missing(source: &Source<'_>, node: &Node, known: &Known<'_>) -> Option<Diagnostic> {
    let written = node.as_str().filter(|text| !text.contains("${") && !text.contains("://"))?;
    let target = assets::resolve(&[], written);
    if target.as_deref().is_some_and(|target| known.paths.contains(target)) {
        return None;
    }
    let label = target.map_or_else(|| "leads out of the project".to_owned(), |target| format!("no file at {target}"));
    let message = format!("`source.openapi` names {written}, which is not in the project");
    let diagnostic =
        Diagnostic::new(Code::V2004, node.span, message).label(label).note("`vc sync` reads the spec from it");
    Some(assets::hint(diagnostic, (source, node, written), known))
}
