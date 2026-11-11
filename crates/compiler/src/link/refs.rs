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
    let same = source.full("");
    let (here, elsewhere): (Vec<&str>, Vec<&str>) =
        kind.table.keys().map(String::as_str).partition(|id| id.starts_with(&same));
    let mut found = nearest(full, here);
    if found.is_empty() {
        found = nearest(full, elsewhere);
    }
    found.truncate(MAX_SUGGESTIONS);
    found
}

/// The `ids` close to `full`. Those in the file `full` names are compared by their own ID alone,
/// so the shared file ID cannot make an unrelated item look close; without that file (a misspelt
/// file ID) whole IDs are compared.
fn nearest<'t>(full: &str, ids: Vec<&'t str>) -> Vec<&'t str> {
    let (file, item) = full.split_once('#').unwrap_or_default();
    let in_file: Vec<(&'t str, &'t str)> =
        ids.iter().filter_map(|id| Some((id.strip_prefix(file)?.strip_prefix('#')?, *id))).collect();
    if in_file.is_empty() {
        return suggest::close(full, ids);
    }
    let items = suggest::close(item, in_file.iter().map(|(own, _)| *own));
    items.into_iter().filter_map(|item| in_file.iter().find(|(own, _)| *own == item).map(|(_, id)| *id)).collect()
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
