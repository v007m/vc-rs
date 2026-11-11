//! References between items: apis and tests (V2003), auths and profiles (V2002), and the
//! report formats (V2302).

use std::collections::{HashMap, HashSet};

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::FileKind;
use vc_parser::Node;

use super::project;
use super::project::{Item, Project, Source};
use crate::suggest;

/// The report files `reports.formats` names besides a reporter plugin's ID.
const REPORT_FORMATS: [&str; 3] = ["ctrf", "junit", "html"];

/// At most this many names in a "did you mean one of" help.
const MAX_SUGGESTIONS: usize = 3;

/// One kind of item a reference names.
struct Kind<'t, 'a> {
    /// `api` or `test`.
    name: &'static str,
    /// The items of that kind, by full ID.
    table: &'t HashMap<String, Item<'a>>,
}

/// Checks every reference in the project.
pub fn check(project: &Project<'_>, diagnostics: &mut Diagnostics) {
    let apis = Kind { name: "api", table: &project.apis };
    let tests = Kind { name: "test", table: &project.tests };
    let globals = Globals::of(project);
    for source in &project.sources {
        for node in callers(source) {
            let cleanup = node.get("cleanup").and_then(|cleanup| cleanup.get("call"));
            for (written, kind) in [(node.get("call"), &apis), (cleanup, &apis), (node.get("test"), &tests)] {
                let Some(written) = written else { continue };
                diagnostics.extend(item(project, (source, written), kind));
            }
            diagnostics.extend(globals.auth(node));
        }
        diagnostics.extend(globals.auth(source.root));
        for profile in project::list(source.root, "profiles").iter().filter(|_| source.kind != FileKind::Config) {
            diagnostics.extend(profile.get("id").and_then(|id| globals.profile(id)));
        }
    }
    if let Some(reports) = project.config().and_then(|config| config.root.get("reports")) {
        diagnostics.extend(project::list(reports, "formats").iter().filter_map(|name| globals.format(name)));
    }
}

/// Every node that may hold `call`, `test`, `cleanup` or `auth`: actions, tests, apis and load
/// mix entries.
fn callers<'a>(source: &Source<'a>) -> Vec<&'a Node> {
    let mut nodes = project::actions(source);
    nodes.extend(["tests", "apis", "mix"].iter().flat_map(|key| project::list(source.root, key)));
    nodes
}

/// V2003 for a reference to an api or test that does not exist.
fn item(project: &Project<'_>, (source, node): (&Source<'_>, &Node), kind: &Kind<'_, '_>) -> Option<Diagnostic> {
    let written = node.as_scalar()?.text.as_str();
    let full = source.resolve(written);
    let file = full.split('#').next().unwrap_or_default();
    if kind.table.contains_key(&full) || project.unreadable.contains(file) {
        return None;
    }
    let message = format!("unknown {} \"{full}\"", kind.name);
    let diagnostic = Diagnostic::new(Code::V2003, node.span, message).label(format!("no {} has this ID", kind.name));
    Some(hint(diagnostic, (source, node, written), &close(source, &full, kind)))
}

/// The IDs close to `full`, those in the same file first.
fn close<'t>(source: &Source<'_>, full: &str, kind: &Kind<'t, '_>) -> Vec<&'t str> {
    let mut ids: Vec<&'t str> = kind.table.keys().map(String::as_str).collect();
    ids.sort_unstable();
    let mut found = nearest(full, &ids);
    let same = source.full("");
    found.sort_by_key(|id| !id.starts_with(&same));
    found.truncate(MAX_SUGGESTIONS);
    found
}

/// The `ids` close to `full`. The file ID and the item's own ID are measured apart, so a shared
/// or misspelt file ID cannot make an unrelated item look close: the items of the file `full`
/// names, or else of the files close to it, are compared by their own ID. When none of those is
/// close, the items of every file are, so an item moved to another file is still found. Only
/// when no file is close (a bare ID resolved to the wrong file) are whole IDs compared.
fn nearest<'t>(full: &str, ids: &[&'t str]) -> Vec<&'t str> {
    let (file, item) = full.split_once('#').unwrap_or_default();
    let files = near_files(file, ids);
    let in_files = own_ids(ids, |own_file| files.contains(&own_file));
    if in_files.is_empty() {
        return suggest::close(full, ids.iter().copied());
    }
    let found = by_own_id(item, &in_files);
    if found.is_empty() { by_own_id(item, &own_ids(ids, |_| true)) } else { found }
}

/// Each of the `ids` whose file is kept, as its own ID and its full ID.
fn own_ids<'t>(ids: &[&'t str], keep: impl Fn(&str) -> bool) -> Vec<(&'t str, &'t str)> {
    let split = |id: &&'t str| id.split_once('#').filter(|(file, _)| keep(file)).map(|(_, own)| (own, *id));
    ids.iter().filter_map(split).collect()
}

/// The full IDs whose own ID is close to `item`, nearest first.
fn by_own_id<'t>(item: &str, pairs: &[(&'t str, &'t str)]) -> Vec<&'t str> {
    let mut found: Vec<&'t str> = Vec::new();
    for close in suggest::close(item, pairs.iter().map(|(own, _)| *own)) {
        for (own, id) in pairs {
            if *own == close && !found.contains(id) {
                found.push(id);
            }
        }
    }
    found
}

/// `file` itself when an ID lives in it, else the file IDs close to it.
fn near_files<'t>(file: &str, ids: &[&'t str]) -> Vec<&'t str> {
    let mut files: Vec<&'t str> = ids.iter().filter_map(|id| id.split_once('#').map(|(own, _)| own)).collect();
    files.sort_unstable();
    files.dedup();
    if let Some(own) = files.iter().copied().find(|own| *own == file) {
        return vec![own];
    }
    suggest::close(file, files)
}

/// The help, and a certain fix when exactly one ID is close.
fn hint(diagnostic: Diagnostic, (source, node, written): (&Source<'_>, &Node, &str), close: &[&str]) -> Diagnostic {
    let [one] = close else {
        if close.is_empty() {
            return diagnostic.help("check the file ID and the item's `id`; another file's item is `fileId#id`");
        }
        return diagnostic.help(format!("did you mean one of {}?", suggest::quoted(close)));
    };
    let short = one.strip_prefix(&source.full("")).filter(|_| !written.contains('#'));
    let text = short.unwrap_or(one);
    let diagnostic = diagnostic.help(format!("did you mean `{one}`?"));
    if node.span.byte_range().len() != written.len() {
        return diagnostic;
    }
    diagnostic.fix(Fix::certain(format!("Replace with {text}"), vec![TextEdit::replace(node.span, text)]))
}

/// The config's auth, profile and plugin IDs.
struct Globals<'a> {
    auth: HashSet<&'a str>,
    profiles: HashSet<&'a str>,
    plugins: HashSet<&'a str>,
    /// Whether the config could be read; without it nothing is reported.
    known: bool,
}

impl<'a> Globals<'a> {
    fn of(project: &Project<'a>) -> Self {
        let config = project.config().filter(|config| !config.broken);
        let ids = |key: &str| -> HashSet<&'a str> {
            let items = config.map(|config| project::list(config.root, key)).unwrap_or_default();
            items.iter().filter_map(|item| item.get("id")?.as_scalar()).map(|id| id.text.as_str()).collect()
        };
        Self { auth: ids("auth"), profiles: ids("profiles"), plugins: ids("plugins"), known: config.is_some() }
    }

    /// V2002 for an `auth` naming no auth; `none` turns auth off.
    fn auth(&self, node: &Node) -> Option<Diagnostic> {
        let auth = node.get("auth")?;
        let id = auth.get("id").unwrap_or(auth);
        let name = id.as_scalar()?.text.as_str();
        (name != "none").then_some(())?;
        self.unknown(id, &self.auth, "auth")
    }

    /// V2002 for a file's `profiles` entry naming no profile.
    fn profile(&self, id: &Node) -> Option<Diagnostic> {
        self.unknown(id, &self.profiles, "profile")
    }

    /// V2302 for a `reports.formats` name that is neither a report format nor a plugin's ID.
    fn format(&self, node: &Node) -> Option<Diagnostic> {
        let name = node.as_scalar()?.text.as_str();
        if !self.known || REPORT_FORMATS.contains(&name) || self.plugins.contains(name) {
            return None;
        }
        let message =
            format!("expected one of {} or a reporter plugin's ID, found `{name}`", REPORT_FORMATS.join(", "));
        let diagnostic = Diagnostic::new(Code::V2302, node.span, message).label("not a report format or plugin");
        let mut plugins: Vec<&str> = self.plugins.iter().copied().collect();
        plugins.sort_unstable();
        let known = [REPORT_FORMATS.as_slice(), &plugins].concat();
        Some(suggest::hint(diagnostic, &suggest::Misspelling { word: name, span: node.span, known: &known }))
    }

    fn unknown(&self, node: &Node, known: &HashSet<&str>, kind: &str) -> Option<Diagnostic> {
        let name = node.as_scalar()?.text.as_str();
        if !self.known || known.contains(name) {
            return None;
        }
        let message = format!("unknown {kind} `{name}`");
        let diagnostic =
            Diagnostic::new(Code::V2002, node.span, message).label(format!("the config has no such {kind}"));
        let mut names: Vec<&str> = known.iter().copied().collect();
        names.sort_unstable();
        if names.is_empty() {
            return Some(diagnostic.help(format!("the config defines no {kind}; add one there first")));
        }
        let misspelling = suggest::Misspelling { word: name, span: node.span, known: &names };
        Some(suggest::hint(diagnostic, &misspelling))
    }
}
