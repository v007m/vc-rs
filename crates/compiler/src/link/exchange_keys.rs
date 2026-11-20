//! What a stream call writes of its messages, against its type's [`StreamShape`] (D376–D380): a
//! `send` on a type that only receives is V2213, and a field its messages lack is V1004, in an
//! `exchange` and in `expect.messages`: on the api, on a call that reaches it, and on a test whose
//! `call` steps inherit them (D123, D124). A type that does not stream takes neither key, which
//! [`protocol_keys`](super::protocol_keys) reports.

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{FileKind, Protocol, StreamShape};
use vc_parser::{Entry, Node, Value};

use super::project;
use super::project::{Project, Source};
use super::protocol_keys::{self, IN_EXPECT, Inherited, ON_THE_TEST};
use crate::matchers;
use crate::suggest::{self, Misspelling};

/// A stream type and the shape of its messages.
type Stream = (Protocol, &'static StreamShape);

/// Every api's and call's `exchange` and `expect.messages` against its type, and each test-level
/// default against the types of the steps that inherit it.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for source in &project.sources {
        let apis = project::list(source.root, "apis").iter().filter(|_| source.kind == FileKind::Api);
        let apis = apis.map(|api| (api, project::protocol(source.root, api)));
        for (node, protocol) in apis.chain(protocol_keys::calls(project, source)) {
            diagnostics.extend(written(node, protocol));
        }
        diagnostics.extend(defaults(project, source));
    }
    diagnostics
}

/// What `node` writes itself, against `protocol`.
fn written(node: &Node, protocol: Protocol) -> Vec<Diagnostic> {
    let mut found = exchange(node.get("exchange"), protocol);
    found.extend(messages(node.get("expect"), protocol));
    found
}

/// The `exchange` and `expect.messages` of each test with `steps`, against each type a `call`
/// step reaches that does not write its own; a problem two types share is reported once, for
/// the first.
fn defaults(project: &Project<'_>, source: &Source<'_>) -> Vec<Diagnostic> {
    let tests = project::list(source.root, "tests").iter().filter(|_| source.kind == FileKind::Test);
    let mut found = Vec::new();
    for test in tests.filter(|test| test.get("call").is_none()) {
        let steps = protocol_keys::call_steps(project, source, test);
        let exchanges = inheriting(&steps, (&ON_THE_TEST, "exchange"));
        let checks = inheriting(&steps, (&IN_EXPECT, "messages"));
        let problems = exchanges.into_iter().flat_map(|protocol| exchange(test.get("exchange"), protocol));
        let problems = problems.chain(checks.into_iter().flat_map(|protocol| messages(test.get("expect"), protocol)));
        for problem in problems {
            let same = |seen: &Diagnostic| seen.code == problem.code && seen.primary.span == problem.primary.span;
            if !found.iter().any(same) {
                found.push(problem);
            }
        }
    }
    found
}

/// The types reached by the `call` steps, of `steps`, that inherit the test's `key` at `place`,
/// each once.
fn inheriting(steps: &[(&Node, Protocol)], (place, key): (&Inherited, &str)) -> Vec<Protocol> {
    let mut protocols = Vec::new();
    for (_, protocol) in steps.iter().filter(|(step, _)| place.inherits(step, key)) {
        if !protocols.contains(protocol) {
            protocols.push(*protocol);
        }
    }
    protocols
}

/// V2213 at each `send` of the `exchange` at `node` when `protocol` only receives, V2306 at an
/// empty `receive` list, and V1004 at each field its `receive` checks that `protocol`'s messages
/// lack.
fn exchange(node: Option<&Node>, protocol: Protocol) -> Vec<Diagnostic> {
    let (Some(node), Some(shape)) = (node, protocol.stream_shape()) else { return Vec::new() };
    let mut found = Vec::new();
    for item in node.items() {
        found.extend(item.entry("send").filter(|_| !shape.sends).map(|send| refused(send, protocol)));
        let checks = match item.get("receive").map(|receive| (receive, &receive.value)) {
            Some((receive, Value::Seq(checks))) if checks.is_empty() => {
                found.push(empty(receive));
                Vec::new()
            }
            Some((_, Value::Seq(checks))) => checks.iter().collect(),
            Some((receive, _)) => vec![receive],
            None => Vec::new(),
        };
        found.extend(checks.into_iter().flat_map(|check| fields(check, (protocol, shape))));
    }
    found
}

/// V1004 at each field the message checks in `expect.messages` write that `protocol`'s
/// messages lack.
fn messages(expect: Option<&Node>, protocol: Protocol) -> Vec<Diagnostic> {
    let (Some(node), Some(shape)) = (expect.and_then(|expect| expect.get("messages")), protocol.stream_shape()) else {
        return Vec::new();
    };
    matchers::message_checks(node).into_iter().flat_map(|check| fields(check, (protocol, shape))).collect()
}

/// V1004 at each field of the message check `check` that the stream's messages lack.
fn fields(check: &Node, (protocol, shape): Stream) -> Vec<Diagnostic> {
    let stray = check.entries().iter().filter(|entry| !shape.fields.contains(&entry.key.as_str()));
    stray.map(|entry| unknown(entry, (protocol, shape))).collect()
}

/// V1004 at a field the stream's messages lack, with the close field's name as its fix.
fn unknown(entry: &Entry, (protocol, shape): Stream) -> Diagnostic {
    let message = format!("{} message has no field `{}`", protocol.named(), entry.key);
    let diagnostic = Diagnostic::new(Code::V1004, entry.key_span, message).label("not a message field");
    suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: shape.fields })
}

/// V2306 at a `receive` list with no check, which would wait for nothing. Which messages were
/// meant is not certain, so there is no fix.
fn empty(receive: &Node) -> Diagnostic {
    Diagnostic::new(Code::V2306, receive.span, "`receive` lists no message check")
        .label("waits for nothing")
        .help("write the messages to wait for, as in `[{type: joined}, {type: history}]`, or remove the item")
}

/// V2213 at a `send` on a stream that only receives. Dropping the message would change what the
/// test does, so there is no fix.
fn refused(send: &Entry, protocol: Protocol) -> Diagnostic {
    let kind = protocol.named();
    Diagnostic::new(Code::V2213, send.key_span, format!("`send` does not apply to {kind} api"))
        .label(format!("{kind} stream only receives"))
        .help("remove the item, or call a `websocket` api to send messages; write only `receive` items here")
}

#[cfg(test)]
#[path = "exchange_keys_tests.rs"]
mod tests;
