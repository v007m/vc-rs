//! The keys each api `type` allows ([`vc_model::PROTOCOL_KEYS`], D342): a key that does not
//! apply to the api's type is V2213, on the api, in its `expect`, or on a call that reaches it
//! (its `expect`, its `poll.until` when its type takes `poll`, and a `cache.check` call nested
//! in it), or in the
//! `expect` of a request written in place (a `flow` auth's steps), which is always `http`; and a
//! type's own block on an api file whose `type` is another.

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Fix, TextEdit};
use vc_model::{AllowedKeys, FileKind, Protocol, ProtocolKeys};

/// The place [`AllowedKeys`] lists keys for.
type At = fn(&AllowedKeys) -> &'static [&'static str];
use vc_parser::{Entry, Node};

use super::build;
use super::project;
use super::project::{Project, Source};
use crate::shapes::{self, Shape};

/// Each type with the keys it allows.
pub type Table = [(Protocol, ProtocolKeys)];

/// The keys a test with `steps` hands each `call` step that does not write its own (D124).
const INHERITED: [&str; 5] = ["auth", "headers", "query", "body", "poll"];

/// Where a test-level default sits: whether a step writes its own, the keys its type takes
/// there, and where the help says to move it.
struct Inherited {
    own: fn(&Node, &str) -> bool,
    at: At,
    onto: &'static str,
}

/// A key of the test itself, such as `body`.
const ON_THE_TEST: Inherited =
    Inherited { own: |step, key| step.get(key).is_some(), at: |keys| keys.call, onto: "onto the steps that send one" };

/// A key of the test's `expect`, such as `errors`, which merges into each step's (D123); a step
/// whose `expect` has `replace: true` inherits none.
const IN_EXPECT: Inherited = Inherited {
    own: |step, key| {
        let expect = step.get("expect");
        expect.is_some_and(|expect| expect.get(key).is_some() || build::flag(shapes::expect(), expect, "replace"))
    },
    at: |keys| keys.expect,
    onto: "into the `expect` of the steps that check one",
};

/// The types with a block of their own on an api file, named after the type (D343).
const FILE_BLOCKS: [Protocol; 2] = [Protocol::Graphql, Protocol::Sse];

/// One place keys are written: the keys the shape knows there, and those the type takes.
struct Place<'k> {
    shape: &'static Shape,
    keys: &'k ProtocolKeys,
    at: At,
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
            let api_keys = Place { shape: shapes::api(), keys, at: |keys| keys.api };
            diagnostics.extend(stray(source, (api, protocol), &api_keys));
            diagnostics.extend(expect(source, (api, protocol), keys));
        }
        for (call, protocol) in calls(project, source) {
            let Some(keys) = allowed(table, protocol) else { continue };
            let call_keys = Place { shape: shapes::request(), keys, at: |keys| keys.call };
            diagnostics.extend(stray(source, (call, protocol), &call_keys));
            diagnostics.extend(expect(source, (call, protocol), keys));
        }
        diagnostics.extend(defaults(project, source, table));
        if let Some(keys) = allowed(table, Protocol::Http) {
            let inline = in_place(source).flat_map(|node| expect(source, (node, Protocol::Http), keys));
            diagnostics.extend(inline);
        }
    }
    diagnostics
}

/// The requests `source` writes in place, by `url`: the steps of its `flow` auths.
fn in_place<'n>(source: &Source<'n>) -> impl Iterator<Item = &'n Node> {
    let flows = project::list(source.root, "auth").iter().filter(|_| source.kind == FileKind::Config);
    flows.flat_map(|auth| project::list(auth, "steps"))
}

/// The keys `protocol` takes; `None` when it is not listed.
fn allowed(table: &Table, protocol: Protocol) -> Option<&ProtocolKeys> {
    table.iter().find(|(listed, _)| *listed == protocol).map(|(_, keys)| keys)
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
    takes(project, (source, node), (table, |keys| keys.call, key))
}

/// Whether the `call` at `node` takes `key` at `at` under the type of the api it reaches.
fn takes(project: &Project<'_>, (source, node): (&Source<'_>, &Node), (table, at, key): (&Table, At, &str)) -> bool {
    let keys = reached(project, source, node).and_then(|protocol| allowed(table, protocol));
    keys.is_none_or(|keys| keys.takes(at, key))
}

/// V2213 at each key a test with `steps` hands its `call` steps (D124), or merges into their
/// `expect` (D123), that reaches a step whose api's type does not take it.
fn defaults(project: &Project<'_>, source: &Source<'_>, table: &Table) -> Vec<Diagnostic> {
    let tests = project::list(source.root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    let mut found = Vec::new();
    for test in tests.filter(|test| test.get("call").is_none()) {
        let keys = test.entries().iter().filter(|entry| INHERITED.contains(&entry.key.as_str()));
        found.extend(keys.filter_map(|entry| refused(project, (source, test), (table, entry, &ON_THE_TEST))));
        let checks = test.get("expect").map(Node::entries).unwrap_or_default().iter().filter(known(shapes::expect()));
        found.extend(checks.filter_map(|entry| refused(project, (source, test), (table, entry, &IN_EXPECT))));
    }
    found
}

/// V2213 at `entry`, a default of `test` at `place`, when a `call` step that does not write its
/// own reaches an api whose type does not take it. Other steps may use it, so there is no fix.
fn refused(
    project: &Project<'_>,
    (source, test): (&Source<'_>, &Node),
    (table, entry, place): (&Table, &Entry, &Inherited),
) -> Option<Diagnostic> {
    let key = entry.key.as_str();
    let calls = project::list(test, "steps").iter().filter(|step| step.get("call").is_some());
    let mut inheriting = calls.filter(|step| !(place.own)(step, key));
    let step = inheriting.find(|step| !takes(project, (source, step), (table, place.at, key)))?;
    let (call, kind) = (step.entry("call")?, reached(project, source, step)?.named());
    let message = format!("`{key}` does not apply to {kind} api");
    Some(
        Diagnostic::new(Code::V2213, entry.key_span, message)
            .label("a test-level default")
            .secondary(call.value.span, format!("{kind} call without its own `{key}`"))
            .help(format!("move `{key}` {}; {kind} call does not take it", place.onto)),
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

/// V2213 for each key of `node`'s `expect` and `poll.until` that `keys` does not allow. A
/// `poll` the type does not take is one V2213 of its own, whose fix removes it whole, so its
/// `until` is not walked as well.
fn expect(source: &Source<'_>, (node, protocol): (&Node, Protocol), keys: &ProtocolKeys) -> Vec<Diagnostic> {
    let polls = node.get("poll").filter(|_| keys.takes(|keys| keys.call, "poll"));
    let until = polls.and_then(|poll| poll.get("until"));
    let place = Place { shape: shapes::expect(), keys, at: |keys| keys.expect };
    node.get("expect").into_iter().chain(until).flat_map(|checks| stray(source, (checks, protocol), &place)).collect()
}

/// V2213 for each key of `node` the shape knows at `place` but the type does not allow.
fn stray(source: &Source<'_>, (node, protocol): (&Node, Protocol), place: &Place<'_>) -> Vec<Diagnostic> {
    let stray = |entry: &&Entry| !place.keys.takes(place.at, &entry.key);
    node.entries()
        .iter()
        .filter(known(place.shape))
        .filter(stray)
        .map(|entry| removed(source, entry, protocol))
        .collect()
}

/// Whether an entry's key is one `shape` knows; one it does not is the shape check's error.
fn known(shape: &'static Shape) -> impl Fn(&&Entry) -> bool {
    move |entry| shape.all_keys().any(|key| key.name == entry.key)
}

/// V2213 at `entry`'s key, with the certain fix that removes its lines.
fn removed(source: &Source<'_>, entry: &Entry, protocol: Protocol) -> Diagnostic {
    let (key, kind) = (entry.key.as_str(), protocol.named());
    let message = format!("`{key}` does not apply to {kind} api");
    let diagnostic = Diagnostic::new(Code::V2213, entry.key_span, message)
        .label(format!("not {kind} key"))
        .help(format!("remove `{key}`; an api's `type` decides which keys apply (see the syntax reference)"));
    remove_fix(diagnostic, source, entry)
}

/// V2213 at each type's own block on an api file whose `type` is another: a `graphql` block on
/// an `http` file is read by none of its apis, a `graphql` api's included.
fn file_blocks(source: &Source<'_>) -> Vec<Diagnostic> {
    let file = project::protocol(source.root, source.root);
    let blocks = FILE_BLOCKS.iter().filter(|block| source.kind == FileKind::Api && **block != file);
    let entries = blocks.filter_map(|block| Some((source.root.entry(block.key())?, *block)));
    entries.map(|block| stray_block(source, block, file)).collect()
}

/// V2213 at `block`'s own entry on a `file`-type api file, with the fix that removes it.
fn stray_block(source: &Source<'_>, (entry, block): (&Entry, Protocol), file: Protocol) -> Diagnostic {
    let (key, kind) = (entry.key.as_str(), file.named());
    let message = format!("`{key}` does not apply to {kind} file");
    let diagnostic = Diagnostic::new(Code::V2213, entry.key_span, message)
        .label(format!("not {kind} file key"))
        .help(format!("remove `{key}`, or set the file's `type: {key}`; only {} file reads it", block.named()));
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
