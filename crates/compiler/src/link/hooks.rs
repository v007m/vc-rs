//! Hook files (D172). Each `function:` reference names a hook file, relative to the file holding
//! it, and one of its exports. The files the references reach, through relative imports too,
//! become `Program::hooks`; the ones not given yet are wanted, so the caller reads them and
//! compiles again.

use std::collections::{BTreeMap, BTreeSet};

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{HookModules, SourceFiles, SourcePath, SourceSpan, import_paths};
use vc_parser::Node;

use super::assets::{self, Known};
use super::project::{self, Project, Source};
use crate::cache::CacheEntry;
use crate::script::{self, Import, Script};
use crate::suggest::{self, Misspelling};

/// The one module a hook may import that is not a file.
const STD: &str = "vibranium:std";
/// The request hook lists on an api file or api.
const HOOK_LISTS: [&str; 2] = ["beforeRequest", "afterResponse"];
/// The help for a file that is not a hook file.
const SUFFIX_HELP: &str = "hook files are ES modules ending in .ts, .mts, .js or .mjs";
/// The help for a module a hook may not import, by the start of its name.
const IMPORT_HELP: [(&str, &str); 2] = [
    ("@vibranium/sdk", "`@vibranium/sdk` holds only types: write `import type { … } from \"@vibranium/sdk\"`"),
    ("node:", "hooks run in the engine, not Node: `vibranium:std` has the helpers a hook may use"),
];
/// The help for any other module a hook may not import.
const OTHER_HELP: &str = "hooks run without packages: put the code in a file in the project and import it by path";

/// What linking the hook files found.
pub struct Hooks {
    /// V2004, V2005 and V2008.
    pub diagnostics: Diagnostics,
    /// The hook files reached, by project path.
    pub modules: HookModules,
    /// The hook files reached but not given.
    pub wanted: Vec<SourcePath>,
}

/// Links every `function:` reference in `project`.
pub fn link(
    project: &Project<'_>,
    (entries, files): (&[CacheEntry], &SourceFiles),
    assets: Option<&BTreeSet<SourcePath>>,
) -> Hooks {
    let mut linker = Linker::new((entries, files), assets);
    for source in &project.sources {
        for node in functions(source) {
            linker.reference(source, node);
        }
    }
    linker.follow();
    linker.finish()
}

/// Every `function` value in `source`: of an action, a request hook, a custom check or a
/// redact rule.
fn functions<'a>(source: &Source<'a>) -> Vec<&'a Node> {
    let mut holders = project::actions(source);
    holders.extend(["tests", "apis"].iter().flat_map(|key| project::list(source.root, key)));
    holders.extend(source.root.get("redact").map(|redact| project::list(redact, "rules")).unwrap_or_default());
    holders.push(source.root);
    let checks: Vec<&Node> =
        holders.iter().filter_map(|holder| holder.get("cache").and_then(|cache| cache.get("check"))).collect();
    holders.extend(checks);
    holders.into_iter().flat_map(references).collect()
}

/// The `function` values `holder` itself holds.
fn references(holder: &Node) -> Vec<&Node> {
    let expects = [holder.get("expect"), holder.get("poll").and_then(|poll| poll.get("until"))];
    let custom = expects.into_iter().flatten().map(|expect| project::list(expect, "custom"));
    let lists = HOOK_LISTS.iter().map(|key| project::list(holder, key)).chain(custom);
    let items = lists.flatten().filter_map(|item| item.get("function"));
    holder.get("function").into_iter().chain(items).collect()
}

/// Where a path leads.
enum Found<'a> {
    /// To a hook file given, stripped.
    Given(String, &'a Script),
    /// To a hook file given that did not strip (V1013 already).
    Broken,
    /// To a hook file listed but not given.
    Wanted,
    /// To a file that is not a hook file.
    NotScript,
    /// To no file in the project.
    Missing(String),
    /// Out of the project.
    Outside,
}

/// One `function: path#export`.
struct Reference<'r, 'a> {
    source: &'r Source<'a>,
    node: &'r Node,
    file: &'r str,
    export: &'r str,
}

/// The hook files known, and what linking found so far.
struct Linker<'a> {
    given: BTreeMap<&'a str, Option<&'a Script>>,
    paths: Option<BTreeSet<&'a str>>,
    files: &'a SourceFiles,
    reached: BTreeSet<String>,
    wanted: BTreeSet<String>,
    diagnostics: Diagnostics,
}

impl<'a> Linker<'a> {
    fn new((entries, files): (&'a [CacheEntry], &'a SourceFiles), assets: Option<&'a BTreeSet<SourcePath>>) -> Self {
        let path = |entry: &'a CacheEntry| files.get(entry.file).map(|file| file.path.as_str());
        let script = |entry: &'a CacheEntry| entry.unit.as_deref().and_then(|unit| unit.script.as_ref());
        let scripts = entries.iter().filter(|entry| path(entry).is_some_and(script::is_script));
        let given = scripts.filter_map(|entry| Some((path(entry)?, script(entry)))).collect();
        let paths = assets.map(|assets| assets::paths(files, assets));
        let (reached, wanted) = (BTreeSet::new(), BTreeSet::new());
        Self { given, paths, files, reached, wanted, diagnostics: Diagnostics::new() }
    }

    /// Where the hook file `written` leads from `folder`.
    fn locate(&mut self, folder: &[&str], written: &str) -> Found<'a> {
        let Some(path) = assets::resolve(folder, written) else { return Found::Outside };
        if !script::is_script(&path) {
            return Found::NotScript;
        }
        self.find(std::slice::from_ref(&path)).unwrap_or(Found::Missing(path))
    }

    /// Where the import `specifier` leads from `folder`, trying each of [`import_paths`].
    fn locate_import(&mut self, folder: &[&str], specifier: &str) -> Found<'a> {
        let Some(path) = assets::resolve(folder, specifier) else { return Found::Outside };
        if let Some(found) = self.find(&import_paths(&path)) {
            return found;
        }
        let listed = self.paths.as_ref().is_some_and(|paths| paths.contains(path.as_str()));
        if listed && !script::is_script(&path) { Found::NotScript } else { Found::Missing(path) }
    }

    /// The first of `candidates` given, else the first listed, which is then wanted.
    fn find(&mut self, candidates: &[String]) -> Option<Found<'a>> {
        for path in candidates {
            match self.given.get(path.as_str()) {
                Some(Some(script)) => return Some(Found::Given(path.clone(), script)),
                Some(None) => return Some(Found::Broken),
                None => {}
            }
        }
        let listed = |path: &&String| self.paths.as_ref().is_none_or(|paths| paths.contains(path.as_str()));
        let path = candidates.iter().filter(|path| script::is_script(path)).find(listed)?.clone();
        self.wanted.insert(path);
        Some(Found::Wanted)
    }

    /// Checks one `function` value: its file (V2004, V2008) and its export (V2005).
    fn reference(&mut self, source: &Source<'_>, node: &Node) {
        let Some(written) = node.as_str() else { return };
        let (file, export) = written.split_once('#').unwrap_or((written, ""));
        let at = Reference { source, node, file, export };
        match self.locate(&assets::folder(source.path), file) {
            Found::Given(path, script) => {
                if !script.stars && !script.exports.iter().any(|name| name == export) {
                    let diagnostic = self.unexported(&at, script);
                    self.diagnostics.push(diagnostic);
                }
                self.reached.insert(path);
            }
            Found::Broken | Found::Wanted => {}
            found => {
                let diagnostic = self.not_found(&at, &found);
                self.diagnostics.push(diagnostic);
            }
        }
    }

    /// V2004 or V2008 for a reference, with the close hook files as help.
    fn not_found(&self, at: &Reference<'_, '_>, found: &Found<'_>) -> Diagnostic {
        let diagnostic = missing(found, (at.node.span, at.file));
        if diagnostic.code != Code::V2004 {
            return diagnostic;
        }
        let scripts = self.paths.iter().flatten().filter(|path| script::is_script(path));
        let paths = scripts.copied().collect();
        let known = Known { paths, folder: assets::folder(at.source.path), what: "hook paths" };
        assets::hint(diagnostic, (at.source, at.node, at.file), &known)
    }

    /// V2005: the file does not export the function named.
    fn unexported(&self, at: &Reference<'_, '_>, script: &Script) -> Diagnostic {
        let known: Vec<&str> = script.exports.iter().map(String::as_str).collect();
        let message = if at.export.is_empty() {
            format!("the reference to {} names no function after the `#`", at.file)
        } else {
            format!("{} exports no function `{}`", at.file, at.export)
        };
        let diagnostic = Diagnostic::new(Code::V2005, at.node.span, message).label("not exported");
        if known.is_empty() {
            return diagnostic
                .help(format!("{} has no value exports; export the function: `export function …`", at.file));
        }
        if at.export.is_empty() {
            return diagnostic.help(format!("name one after a `#`: {}", suggest::quoted(&known)));
        }
        let span = self.export_span(at).unwrap_or(at.node.span);
        suggest::hint(diagnostic, &Misspelling { word: at.export, span, known: &known })
    }

    /// The span of the export after the `#`, when the scalar is written plain or quoted.
    fn export_span(&self, at: &Reference<'_, '_>) -> Option<SourceSpan> {
        let start = at.node.span.start.offset;
        let quoted = matches!(at.source.text.get(start..=start), Some("'" | "\""));
        let range = at.source.word_at(start + usize::from(quoted) + at.file.len() + 1, at.export)?;
        let position = |offset| self.files.position(at.source.file, offset);
        Some(SourceSpan::new(at.source.file, position(range.start)?, position(range.end)?))
    }

    /// Follows the imports of every hook file reached, once each.
    fn follow(&mut self) {
        let mut queue: Vec<String> = self.reached.iter().cloned().collect();
        while let Some(path) = queue.pop() {
            let Some(Some(script)) = self.given.get(path.as_str()).copied() else { continue };
            for import in &script.imports {
                let next = self.import(&assets::folder(&path), import);
                queue.extend(next.filter(|next| self.reached.insert(next.clone())));
            }
        }
    }

    /// Checks one import (V2004, V2008); the hook file it reaches, if given.
    fn import(&mut self, folder: &[&str], import: &Import) -> Option<String> {
        let specifier = import.specifier.as_str();
        if specifier == STD && !import.dynamic {
            return None;
        }
        if import.dynamic || !(specifier.starts_with("./") || specifier.starts_with("../")) {
            self.diagnostics.push(not_allowed(import));
            return None;
        }
        match self.locate_import(folder, specifier) {
            Found::Given(path, _) => Some(path),
            Found::Broken | Found::Wanted => None,
            found => {
                let help = "an import path is relative to the file that imports it";
                self.diagnostics.push(missing(&found, (import.span, specifier)).help(help));
                None
            }
        }
    }

    fn finish(self) -> Hooks {
        let module = |path: &String| Some((path.clone(), self.given.get(path.as_str()).copied()??.module.clone()));
        let modules = HookModules::new(self.reached.iter().filter_map(module).collect());
        let wanted = self.wanted.iter().filter_map(|path| SourcePath::new(path).ok()).collect();
        Hooks { diagnostics: self.diagnostics, modules, wanted }
    }
}

/// V2008 for a path to a file that is not a hook file, or V2004 for a path to no file.
fn missing(found: &Found<'_>, (span, written): (SourceSpan, &str)) -> Diagnostic {
    match found {
        Found::NotScript => {
            let message = format!("{written} is not a TypeScript or JavaScript module");
            Diagnostic::new(Code::V2008, span, message).label("not a hook file").help(SUFFIX_HELP)
        }
        Found::Missing(path) => {
            let message = format!("{written} is not in the project");
            Diagnostic::new(Code::V2004, span, message).label(format!("no file at {path}"))
        }
        Found::Given(..) | Found::Broken | Found::Wanted | Found::Outside => {
            let message = format!("{written} is not in the project");
            Diagnostic::new(Code::V2004, span, message).label("leads out of the project")
        }
    }
}

/// V2008 for an import a hook may not make.
fn not_allowed(import: &Import) -> Diagnostic {
    if import.dynamic {
        let diagnostic = Diagnostic::new(Code::V2008, import.span, "a hook cannot load a module with `import()`");
        return diagnostic.label("dynamic import").help("import it at the top of the file instead");
    }
    let specifier = &import.specifier;
    let message = format!("a hook can import only files, by relative path, and `{STD}`, not {specifier}");
    let help = IMPORT_HELP.iter().find(|(start, _)| specifier.starts_with(start)).map_or(OTHER_HELP, |(_, help)| help);
    Diagnostic::new(Code::V2008, import.span, message).label("not allowed").help(help)
}

#[cfg(test)]
#[path = "hooks_tests.rs"]
mod tests;
