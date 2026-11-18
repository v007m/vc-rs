//! The keys each api `type` allows ([`vc_model::PROTOCOL_KEYS`], D342): a key that does not
//! apply to the api's type is V2213, on the api, in its `expect`, or on a call that reaches it.

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{FileKind, Protocol, ProtocolKeys};
use vc_parser::{Entry, Node};

use super::project;
use super::project::{Project, Source};
use crate::shapes::{self, Shape};

/// Each type with the keys it allows.
pub type Table = [(Protocol, ProtocolKeys)];

/// One place keys are written: the keys the shape knows there, and those the type allows.
struct Place<'k> {
    shape: &'static Shape,
    allowed: &'k [&'k str],
}

/// Every api, its `expect`, and every call that reaches an api, against the keys of its type.
/// A key the shape does not know is the shape check's error, so it is left out here.
pub fn check(project: &Project<'_>, table: &Table) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for source in &project.sources {
        for api in project::list(source.root, "apis").iter().filter(|_| source.kind == FileKind::Api) {
            let protocol = project::protocol(source.root, api);
            let Some(keys) = allowed(table, protocol) else { continue };
            let api_keys = Place { shape: shapes::api(), allowed: keys.api };
            diagnostics.extend(stray(source, (api, protocol), &api_keys));
            diagnostics.extend(expect(source, (api, protocol), keys));
        }
        for (call, protocol) in calls(project, source) {
            let Some(keys) = allowed(table, protocol) else { continue };
            let call_keys = Place { shape: shapes::request(), allowed: keys.call };
            diagnostics.extend(stray(source, (call, protocol), &call_keys));
            diagnostics.extend(expect(source, (call, protocol), keys));
        }
    }
    diagnostics
}

fn allowed(table: &Table, protocol: Protocol) -> Option<&ProtocolKeys> {
    table.iter().find(|(listed, _)| *listed == protocol).map(|(_, keys)| keys)
}

/// The tests and actions of `source` that `call` an api, each with that api's type.
fn calls<'n>(project: &Project<'n>, source: &Source<'n>) -> Vec<(&'n Node, Protocol)> {
    let tests = project::list(source.root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    let callers = tests.chain(project::actions(source));
    let reached = |node: &'n Node| {
        let item = project.apis.get(&source.resolve(project::text(node, "call")?))?;
        Some((node, project::protocol(project.source(*item).root, item.node)))
    };
    callers.filter_map(reached).collect()
}

/// V2213 for each key of `node`'s `expect` that `keys` does not allow.
fn expect(source: &Source<'_>, (node, protocol): (&Node, Protocol), keys: &ProtocolKeys) -> Vec<Diagnostic> {
    let Some(checks) = node.get("expect") else { return Vec::new() };
    stray(source, (checks, protocol), &Place { shape: shapes::expect(), allowed: keys.expect })
}

/// V2213 for each key of `node` the shape knows at `place` but the type does not allow.
fn stray(source: &Source<'_>, (node, protocol): (&Node, Protocol), place: &Place<'_>) -> Vec<Diagnostic> {
    let known = |entry: &&Entry| place.shape.all_keys().any(|key| key.name == entry.key);
    let stray = |entry: &&Entry| !place.allowed.contains(&entry.key.as_str());
    node.entries().iter().filter(known).filter(stray).map(|entry| removed(source, entry, protocol)).collect()
}

/// V2213 at `entry`'s key, with the certain fix that removes its lines.
fn removed(source: &Source<'_>, entry: &Entry, protocol: Protocol) -> Diagnostic {
    let (key, kind) = (entry.key.as_str(), protocol.key());
    let message = format!("`{key}` does not apply to a `{kind}` api");
    let diagnostic = Diagnostic::new(Code::V2213, entry.key_span, message)
        .label(format!("not a `{kind}` key"))
        .help(format!("remove `{key}`; an api's `type` decides which keys apply (see the syntax reference)"));
    let Some(range) = project::whole_lines(source.text, entry) else { return diagnostic };
    let edit = TextEdit { file: source.file, range, new_text: String::new() };
    diagnostic.fix(Fix::certain(format!("Remove `{key}`"), vec![edit]))
}

#[cfg(test)]
#[path = "protocol_keys_tests.rs"]
mod tests;
