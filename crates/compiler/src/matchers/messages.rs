//! What a stream call's messages are checked with (D379, D380): a `receive` check holds on one
//! message, `expect.messages` on the list of them. A message check is always a map of the
//! message's fields, never a matcher, since `type` is a websocket message's field as well as a
//! matcher: each field it writes, a literal or a matcher, must hold on that field (D360), and
//! `data` takes `match` (paths into the data, as `body.match`) or `equals` (the whole data, as
//! `body.equals`). On the list a map must name a matcher: `some`, `every`, `none` or `length`.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Matcher, MatcherArg, Path};
use vc_parser::{Entry, Node, Value};

use super::errors::{call, one};
use super::{Arg, Unit, calls, find, inner};
use crate::parts::{At, Cx};
use crate::suggest::{self, Misspelling};
use crate::value;

/// What `data` takes.
const DATA: [&str; 2] = ["match", "equals"];

/// The matcher `at` holds: on the list (`Unit::Messages`) or on one message.
pub fn expected(cx: &mut Cx<'_>, at: At<'_>, unit: Unit) -> Option<Matcher> {
    match (&at.node.value, unit) {
        (Value::Map(entries), Unit::Messages) if entries.iter().any(|entry| find(&entry.key).is_some()) => {
            calls(cx, at, (entries, unit))
        }
        (_, Unit::Messages) => on_the_list(cx, at),
        (Value::Map(entries), _) => message(cx, at, entries),
        _ => not_a_message(cx, at),
    }
}

/// The message checks written in `node`, an `expect.messages` value: each one under `every`,
/// `some` or `none`, reached through `not`, `allOf` and `anyOf`; the link stage checks their
/// fields against the type's (D380).
pub fn checks(node: &Node) -> Vec<&Node> {
    let mut found = Vec::new();
    for entry in node.entries() {
        let Some(kind) = find(&entry.key) else { continue };
        match (kind.arg, inner(Unit::Messages, kind)) {
            (Arg::Nested, Unit::Message) => found.push(&entry.value),
            (Arg::Nested, _) => found.extend(checks(&entry.value)),
            (Arg::Several, _) => found.extend(entry.value.items().iter().flat_map(checks)),
            _ => {}
        }
    }
    found
}

/// V2306 for what is written on the list but names no matcher: a message check there would be
/// matched against the whole list, and which messages were meant is not certain, so no fix.
fn on_the_list(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let message = "`messages` takes a matcher on the list of messages";
    let diagnostic = Diagnostic::new(Code::V2306, at.node.span, message).label("names no matcher").help(
        "write `some:` to expect one among them (`{some: {data: {match: {type: done}}}}`), `every:`, `none:` or \
         `length:`",
    );
    cx.push(diagnostic);
    None
}

/// V2306 for what stands where one message's check goes but is not a map.
fn not_a_message(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let message = "a message check is a map of the message's fields";
    let diagnostic = Diagnostic::new(Code::V2306, at.node.span, message)
        .label("not a message check")
        .help("write the fields to match, as in `{event: created}` or `{data: {match: {id: 7}}}`");
    cx.push(diagnostic);
    None
}

/// A message check, as the internal `fields` matcher.
fn message(cx: &mut Cx<'_>, at: At<'_>, entries: &[Entry]) -> Option<Matcher> {
    let found: Vec<_> = entries.iter().map(|entry| field(cx, at, entry)).collect();
    let fields = found.into_iter().collect::<Option<Vec<_>>>()?;
    Some(one(call(("fields", MatcherArg::Fields(fields)), at.node.span)))
}

/// One written field: `data` as its `match` or `equals`, any other a literal or a matcher.
fn field(cx: &mut Cx<'_>, at: At<'_>, entry: &Entry) -> Option<(String, Matcher)> {
    let at = at.child(&entry.value);
    if entry.key != "data" {
        return Some((entry.key.clone(), super::matcher(cx, at, Unit::Plain)?));
    }
    let check = match &at.node.value {
        Value::Map(checks) => match checks.as_slice() {
            [check] => data(cx, at, check),
            [] => not_one(cx, at, checks),
            _ => several(cx, at, checks),
        },
        _ => data_not_a_map(cx, at),
    };
    Some(("data".to_owned(), check?))
}

/// V2306 for a `data` that writes neither `match` nor `equals`, or both: `equals` fixes the whole
/// data, so which one was meant is not certain and there is no fix.
fn not_one(cx: &mut Cx<'_>, at: At<'_>, checks: &[Entry]) -> Option<Matcher> {
    let second = checks.iter().filter(|check| DATA.contains(&check.key.as_str())).nth(1);
    let (message, span) = second.map_or_else(
        || ("`data` checks nothing", at.node.span),
        |second| ("`data` takes `match` or `equals`, not both", second.key_span),
    );
    let diagnostic = Diagnostic::new(Code::V2306, span, message)
        .label("one data check")
        .help("write `data: {match: {…}}` for paths into the data, or `data: {equals: …}` for the whole of it");
    cx.push(diagnostic);
    None
}

/// A `data` that writes more than one check: each is lowered, so a stray key is its own V1004
/// and the errors inside the others still show; `match` and `equals` both is also V2306.
fn several(cx: &mut Cx<'_>, at: At<'_>, checks: &[Entry]) -> Option<Matcher> {
    for check in checks {
        drop(data(cx, at, check));
    }
    let known = checks.iter().filter(|check| DATA.contains(&check.key.as_str())).count();
    if known > 1 { not_one(cx, at, checks) } else { None }
}

/// V2302 for `data` written as a value: it takes `match` or `equals`.
fn data_not_a_map(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let message = format!("`data` takes `match` or `equals`, found {}", crate::shape_check::found(at.node));
    let diagnostic = Diagnostic::new(Code::V2302, at.node.span, message)
        .label("not a map")
        .help("write `data: {equals: …}` for the whole data, or `data: {match: {…}}` for paths into it");
    cx.push(diagnostic);
    None
}

/// `match` or `equals` under `data`.
fn data(cx: &mut Cx<'_>, at: At<'_>, check: &Entry) -> Option<Matcher> {
    match check.key.as_str() {
        "match" => paths(cx, at.child(&check.value)),
        "equals" => value::lower(cx, at.child(&check.value)).map(Matcher::Equals),
        _ => {
            let message = format!("`data` has no `{}`", check.key);
            let diagnostic = Diagnostic::new(Code::V1004, check.key_span, message)
                .label("not a data check")
                .help("`data` takes `match` (paths into the data, as `{match: {id: 7}}`) or `equals` (the whole data)");
            let free = unwritten(at.node);
            cx.push(suggest::hint(diagnostic, &Misspelling { word: &check.key, span: check.key_span, known: &free }));
            None
        }
    }
}

/// The data checks `data` does not write yet: a fix never renames a key to one already there.
fn unwritten(data: &Node) -> Vec<&'static str> {
    let written = |key: &&str| data.entries().iter().any(|entry| entry.key == *key);
    DATA.into_iter().filter(|key| !written(key)).collect()
}

/// `data.match`, as the internal `paths` matcher.
fn paths(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let Value::Map(entries) = &at.node.value else {
        let message = format!("`data.match` takes a map of paths, found {}", crate::shape_check::found(at.node));
        let diagnostic = Diagnostic::new(Code::V2302, at.node.span, message)
            .label("not a map")
            .help("write each path into the data with its matcher, as in `{match: {user.id: 7}}`");
        cx.push(diagnostic);
        return None;
    };
    let found: Vec<_> = entries.iter().map(|entry| path(cx, at, entry)).collect();
    let paths = found.into_iter().collect::<Option<Vec<_>>>()?;
    Some(one(call(("paths", MatcherArg::Paths(paths)), at.node.span)))
}

/// One path and its matcher, which sees a list when the path holds `[*]`.
fn path(cx: &mut Cx<'_>, at: At<'_>, entry: &Entry) -> Option<(Path, Matcher)> {
    let matcher = super::matcher(cx, at.child(&entry.value), Unit::Plain)?;
    if entry.key.contains("[*]") {
        super::on_list(cx, &matcher, &entry.key);
    }
    Some((crate::link::body_path(&entry.key), matcher))
}
