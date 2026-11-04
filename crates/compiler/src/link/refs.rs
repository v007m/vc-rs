//! References between items: apis and tests (V2003), auths and profiles (V2002).

use std::collections::{HashMap, HashSet};

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::FileKind;
use vc_parser::Node;

use super::project;
use super::project::{Item, Project, Source};
use crate::suggest;

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
    let mut found = suggest::close(full, here);
    if found.is_empty() {
        found = suggest::close(full, elsewhere);
    }
    found.truncate(MAX_SUGGESTIONS);
    found
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

/// The config's auth and profile IDs.
struct Globals<'a> {
    auth: HashSet<&'a str>,
    profiles: HashSet<&'a str>,
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
        Self { auth: ids("auth"), profiles: ids("profiles"), known: config.is_some() }
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
