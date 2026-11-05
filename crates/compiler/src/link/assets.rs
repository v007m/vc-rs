//! `$file` paths (V2004): each names a file in the project, relative to the file holding it.
//! A path holding a template is only known at run time, and a `#/pointer` after the path
//! points inside the file, so neither is checked here.

use std::collections::BTreeSet;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{SourceFileId, SourceFiles, SourcePath};
use vc_parser::{Node, Value};

use super::project::{Project, Source};
use crate::suggest;

/// At most this many paths in a "did you mean one of" help.
const MAX_SUGGESTIONS: usize = 3;

/// The project's files, and the folder of the file being checked.
struct Known<'k> {
    paths: BTreeSet<&'k str>,
    folder: Vec<&'k str>,
}

/// Checks every `$file` path against the definition files and `assets`.
pub fn check(project: &Project<'_>, files: &SourceFiles, assets: &BTreeSet<SourcePath>) -> Diagnostics {
    let mut paths: BTreeSet<&str> = assets.iter().map(SourcePath::as_str).collect();
    let numbered = (0..).map(SourceFileId::new).map_while(|id| files.get(id));
    paths.extend(numbered.map(|file| file.path.as_str()));
    let mut known = Known { paths, folder: Vec::new() };
    let mut diagnostics = Diagnostics::new();
    for source in &project.sources {
        let Some(file) = files.get(source.file) else { continue };
        known.folder = folder(file.path.as_str());
        let mut found = Vec::new();
        file_refs(source.root, &mut found);
        diagnostics.extend(found.into_iter().filter_map(|node| missing(source, node, &known)));
    }
    diagnostics
}

/// The folders `path` is in, outermost first.
pub fn folder(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop();
    parts
}

/// Every `$file` value under `node`: the value of a map whose only key is `$file`.
fn file_refs<'n>(node: &'n Node, found: &mut Vec<&'n Node>) {
    match &node.value {
        Value::Map(entries) => match entries.as_slice() {
            [only] if only.key == "$file" => found.push(&only.value),
            _ => entries.iter().for_each(|entry| file_refs(&entry.value, found)),
        },
        Value::Seq(items) => items.iter().for_each(|item| file_refs(item, found)),
        Value::Scalar(_) => {}
    }
}

/// The path `written` names from `folder`, or `None` when it leads out of the project.
pub fn resolve(folder: &[&str], written: &str) -> Option<String> {
    let mut parts = folder.to_vec();
    for part in written.split('/').filter(|part| !part.is_empty() && *part != ".") {
        if part == ".." {
            parts.pop()?;
        } else {
            parts.push(part);
        }
    }
    Some(parts.join("/"))
}

/// `path` as written from `folder`: `../data/user.json`.
fn relative(folder: &[&str], path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let shared = folder.iter().zip(&parts).take_while(|(a, b)| a == b).count();
    let mut written: Vec<&str> = vec![".."; folder.len() - shared];
    written.extend(&parts[shared..]);
    written.join("/")
}

/// V2004 when the `$file` at `node` names no file in the project.
fn missing(source: &Source<'_>, node: &Node, known: &Known<'_>) -> Option<Diagnostic> {
    let text = node.as_str().filter(|text| !text.contains("${"))?;
    let written = text.split('#').next().unwrap_or_default();
    let target = resolve(&known.folder, written);
    if target.as_deref().is_some_and(|target| known.paths.contains(target)) {
        return None;
    }
    let label = target.map_or_else(|| "leads out of the project".to_owned(), |target| format!("no file at {target}"));
    let message = format!("`$file` names {written}, which is not in the project");
    let diagnostic = Diagnostic::new(Code::V2004, node.span, message).label(label);
    Some(hint(diagnostic, (source, node, written), known))
}

/// The close paths as help, with a certain fix when exactly one is close.
fn hint(diagnostic: Diagnostic, (source, node, written): (&Source<'_>, &Node, &str), known: &Known<'_>) -> Diagnostic {
    let candidates: Vec<String> = known.paths.iter().map(|path| relative(&known.folder, path)).collect();
    let mut close = suggest::close(written, candidates.iter().map(String::as_str));
    close.truncate(MAX_SUGGESTIONS);
    match close.as_slice() {
        [] => {
            let place = if known.folder.is_empty() { "the project root".to_owned() } else { known.folder.join("/") };
            diagnostic.help(format!("`$file` paths are relative to the file that holds them, here {place}"))
        }
        [one] => {
            let diagnostic = diagnostic.help(format!("did you mean `{one}`?"));
            match path_range(source, node, written) {
                Some(range) => {
                    let edit = TextEdit { file: source.file, range, new_text: (*one).to_owned() };
                    diagnostic.fix(Fix::certain(format!("Replace with {one}"), vec![edit]))
                }
                None => diagnostic,
            }
        }
        several => diagnostic.help(format!("did you mean one of {}?", suggest::quoted(several))),
    }
}

/// The bytes of the path in the file, when the scalar is written plain or quoted without escapes.
fn path_range(source: &Source<'_>, node: &Node, written: &str) -> Option<std::ops::Range<usize>> {
    let start = node.span.start.offset;
    let quoted = matches!(source.text.get(start..=start), Some("'" | "\""));
    source.word_at(start + usize::from(quoted), written)
}

#[cfg(test)]
mod tests {
    use super::{folder, relative, resolve};

    #[test]
    fn paths_resolve_from_the_folder_and_stay_in_the_project() {
        let here = folder("services/users/crud.test.yaml");
        assert_eq!(here, ["services", "users"]);
        assert_eq!(resolve(&here, "data/u.json").as_deref(), Some("services/users/data/u.json"));
        assert_eq!(resolve(&here, "./../../openapi//u.yaml").as_deref(), Some("openapi/u.yaml"));
        assert_eq!(resolve(&here, "../../../u.json"), None);
        assert_eq!(resolve(&folder("x.api.yaml"), "u.json").as_deref(), Some("u.json"));
    }

    #[test]
    fn paths_are_written_back_from_the_folder() {
        let here = ["services", "users"];
        assert_eq!(relative(&here, "services/users/data/u.json"), "data/u.json");
        assert_eq!(relative(&here, "openapi/u.yaml"), "../../openapi/u.yaml");
        assert_eq!(relative(&here, "services/orders/o.json"), "../orders/o.json");
        assert_eq!(relative(&[], "a/b.json"), "a/b.json");
    }
}
