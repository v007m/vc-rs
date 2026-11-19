//! The keys each api `type` allows ([`vc_model::PROTOCOL_KEYS`], D342): a key that does not
//! apply to the api's type is V2213, on the api, in its `expect`, or on a call that reaches it
//! (its `expect` and `poll.until` too, and a `cache.check` call nested in it); and a type's own
//! block on an api file whose `type` is another.

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{AllowedKeys, FileKind, Protocol, ProtocolKeys};
use vc_parser::{Entry, Node};

use super::project;
use super::project::{Project, Source};
use crate::shapes::{self, Shape};

/// Each type with the keys it allows.
pub type Table = [(Protocol, ProtocolKeys)];

/// The keys a test with `steps` hands each `call` step that does not write its own (D124).
const INHERITED: [&str; 5] = ["auth", "headers", "query", "body", "poll"];

/// The types with a block of their own on an api file, named after the type (D343).
const FILE_BLOCKS: [Protocol; 1] = [Protocol::Graphql];

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
        diagnostics.extend(file_blocks(source));
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
        diagnostics.extend(defaults(project, source, table));
    }
    diagnostics
}

/// The keys `protocol` restricts itself to; `None` when it allows every key, or is not listed.
fn allowed(table: &Table, protocol: Protocol) -> Option<&AllowedKeys> {
    let (_, keys) = table.iter().find(|(listed, _)| *listed == protocol)?;
    match keys {
        ProtocolKeys::Only(keys) => Some(keys),
        ProtocolKeys::All => None,
    }
}

/// The type of the api the `call` at `node`, written in `source`, reaches.
fn reached(project: &Project<'_>, source: &Source<'_>, node: &Node) -> Option<Protocol> {
    let item = project.apis.get(&source.resolve(project::text(node, "call")?))?;
    Some(project::protocol(project.source(*item).root, item.node))
}

/// Whether the `call` at `node` takes `key` under the type of the api it reaches. A call that
/// reaches no api takes every key, since V2003 reports it. The build asks this too before a step
/// inherits a test's key, so a `graphql` call never sends a test-level `body` (D343).
pub fn call_takes(project: &Project<'_>, (source, node): (&Source<'_>, &Node), (table, key): (&Table, &str)) -> bool {
    let keys = reached(project, source, node).and_then(|protocol| allowed(table, protocol));
    keys.is_none_or(|keys| keys.call.contains(&key))
}

/// V2213 at each key a test with `steps` hands its `call` steps (D124) that reaches a step whose
/// api's type does not take it.
fn defaults(project: &Project<'_>, source: &Source<'_>, table: &Table) -> Vec<Diagnostic> {
    let tests = project::list(source.root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    let mut found = Vec::new();
    for test in tests.filter(|test| test.get("call").is_none()) {
        let keys = test.entries().iter().filter(|entry| INHERITED.contains(&entry.key.as_str()));
        found.extend(keys.filter_map(|entry| refused(project, (source, test), (table, entry))));
    }
    found
}

/// V2213 at `entry`, a key of `test`, when a `call` step that does not write its own reaches an
/// api whose type does not take it. Other steps may send it, so there is no fix.
fn refused(
    project: &Project<'_>,
    (source, test): (&Source<'_>, &Node),
    (table, entry): (&Table, &Entry),
) -> Option<Diagnostic> {
    let key = entry.key.as_str();
    let calls = project::list(test, "steps").iter().filter(|step| step.get("call").is_some());
    let step =
        calls.filter(|step| step.get(key).is_none()).find(|step| !call_takes(project, (source, step), (table, key)))?;
    let (call, kind) = (step.entry("call")?, reached(project, source, step)?.key());
    let message = format!("`{key}` does not apply to a `{kind}` api");
    Some(
        Diagnostic::new(Code::V2213, entry.key_span, message)
            .label("a test-level default")
            .secondary(call.value.span, format!("a `{kind}` call without its own `{key}`"))
            .help(format!("move `{key}` onto the steps that send one; a `{kind}` call does not take it")),
    )
}

/// The tests and actions of `source` that `call` an api, each with that api's type, nested
/// `cache.check` actions included.
fn calls<'n>(project: &Project<'n>, source: &Source<'n>) -> Vec<(&'n Node, Protocol)> {
    let tests = project::list(source.root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    let callers = tests.chain(project::actions(source)).flat_map(nested);
    let reached = |node: &'n Node| Some((node, reached(project, source, node)?));
    callers.filter_map(reached).collect()
}

/// `action`, then each `cache.check` action nested in it, the innermost last.
fn nested(action: &Node) -> impl Iterator<Item = &Node> {
    std::iter::successors(Some(action), |node| node.get("cache")?.get("check"))
}

/// V2213 for each key of `node`'s `expect` and `poll.until` that `keys` does not allow.
fn expect(source: &Source<'_>, (node, protocol): (&Node, Protocol), keys: &AllowedKeys) -> Vec<Diagnostic> {
    let until = node.get("poll").and_then(|poll| poll.get("until"));
    let place = Place { shape: shapes::expect(), allowed: keys.expect };
    node.get("expect").into_iter().chain(until).flat_map(|checks| stray(source, (checks, protocol), &place)).collect()
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
    remove_fix(diagnostic, source, entry)
}

/// V2213 at each type's own block on an api file whose `type` is another: a `graphql` block on
/// an `http` file is read by none of its apis, a `graphql` api's included.
fn file_blocks(source: &Source<'_>) -> Vec<Diagnostic> {
    let file = project::protocol(source.root, source.root);
    let blocks = FILE_BLOCKS.iter().filter(|block| source.kind == FileKind::Api && **block != file);
    let entries = blocks.filter_map(|block| source.root.entry(block.key()));
    entries.map(|entry| stray_block(source, entry, file)).collect()
}

/// V2213 at the block `entry` on a `file`-type api file, with the fix that removes it.
fn stray_block(source: &Source<'_>, entry: &Entry, file: Protocol) -> Diagnostic {
    let (key, kind) = (entry.key.as_str(), file.key());
    let message = format!("`{key}` does not apply to a `{kind}` file");
    let diagnostic = Diagnostic::new(Code::V2213, entry.key_span, message)
        .label(format!("not a `{kind}` file key"))
        .help(format!("remove `{key}`, or set the file's `type: {key}`; only a `{key}` file reads it"));
    remove_fix(diagnostic, source, entry)
}

/// `diagnostic` with the certain fix that removes `entry`'s lines, when they are its own.
fn remove_fix(diagnostic: Diagnostic, source: &Source<'_>, entry: &Entry) -> Diagnostic {
    let Some(range) = project::whole_lines(source.text, entry) else { return diagnostic };
    let edit = TextEdit { file: source.file, range, new_text: String::new() };
    diagnostic.fix(Fix::certain(format!("Remove `{}`", entry.key), vec![edit]))
}

#[cfg(test)]
#[path = "protocol_keys_tests.rs"]
mod tests;
