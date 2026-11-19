//! What `expect.errors` expects of a `graphql` response's errors (D345), as matchers on the
//! normalised list: a list of expected errors, each paired with its own error (D362), or
//! matchers on the list. Under `every`, `some` and `none` a map
//! that names no matcher is an expected error (`{ some: { code: TIMEOUT } }`); on the list itself
//! it is V2306, since the list is no error. An expected error lowers to the internal `fields`
//! matcher (D360), and the list form to the internal `entries` matcher, which holds it once.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::{Matcher, MatcherArg, MatcherCall, Segment, SourceSpan};
use vc_parser::{Entry, Node, ScalarKind, Value};

use super::{Unit, calls, find};
use crate::parts::{At, Cx};
use crate::suggest::{self, Misspelling};

/// The keys an entry may write: `code` is short for `extensions.code`.
const KEYS: [&str; 4] = ["message", "path", "code", "extensions"];

/// The matcher `at` holds under `expect.errors`: on the list (`Unit::Errors`) or on one error.
pub fn expected(cx: &mut Cx<'_>, at: At<'_>, unit: Unit) -> Option<Matcher> {
    match (&at.node.value, unit) {
        (Value::Map(entries), _) if entries.iter().any(|entry| find(&entry.key).is_some()) => {
            calls(cx, at, (entries, unit))
        }
        (Value::Seq(items), Unit::Errors) => listed(cx, at, items),
        (Value::Map(_), Unit::Errors) => on_the_list(cx, at),
        (Value::Map(entries), _) => entry(cx, at, entries),
        (_, Unit::Errors) => not_a_check(cx, at),
        _ => not_an_entry(cx, at),
    }
}

/// The list form, as the internal `entries` matcher.
fn listed(cx: &mut Cx<'_>, at: At<'_>, items: &[Node]) -> Option<Matcher> {
    let found: Vec<_> = items.iter().map(|item| item_entry(cx, at.child(item))).collect();
    let entries = found.into_iter().collect::<Option<Vec<_>>>()?;
    Some(one(call(("entries", MatcherArg::Matchers(entries)), at.node.span)))
}

/// V2306 for an expected error written where the whole list is matched. Whether one error or
/// exactly these was meant is not certain, so there is no fix.
fn on_the_list(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let message = "an expected error here would be matched against the whole list of errors";
    let diagnostic = Diagnostic::new(Code::V2306, at.node.span, message).label("names no matcher").help(
        "write `some:` to expect it among the errors (`{some: {code: FORBIDDEN}}`), or a list to expect exactly \
         these (`[{code: FORBIDDEN}]`)",
    );
    cx.push(diagnostic);
    None
}

/// V2306 for a value on the list that is none of `none`, `any`, a list or a matcher: `null`,
/// `false` or `0` would be compared with the list and never hold.
fn not_a_check(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let message = "`errors` takes `none`, `any`, a list of expected errors or a matcher on the list";
    let diagnostic = Diagnostic::new(Code::V2306, at.node.span, message)
        .label("never matches")
        .help("write `none` to expect no errors, `any` for at least one, or a list (`[{code: FORBIDDEN}]`)");
    cx.push(diagnostic);
    None
}

/// One item of the list form, which must be an entry.
fn item_entry(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    if let Value::Map(entries) = &at.node.value {
        return entry(cx, at, entries);
    }
    not_an_entry(cx, at)
}

/// V2306 for what stands where one expected error goes but is not a map.
fn not_an_entry(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let message = "an expected error is a map of `message`, `path`, `code` and `extensions`";
    cx.push(Diagnostic::new(Code::V2306, at.node.span, message).label("not an error entry"));
    None
}

/// An entry: each key it writes, a literal or a matcher, must hold on that field.
fn entry(cx: &mut Cx<'_>, at: At<'_>, entries: &[Entry]) -> Option<Matcher> {
    let found: Vec<_> = entries.iter().map(|entry| field(cx, at, entry)).collect();
    let fields = found.into_iter().collect::<Option<Vec<_>>>()?;
    Some(one(call(("fields", MatcherArg::Fields(fields)), at.node.span)))
}

fn field(cx: &mut Cx<'_>, at: At<'_>, entry: &Entry) -> Option<(String, Matcher)> {
    if !KEYS.contains(&entry.key.as_str()) {
        let message = format!("an expected error has no `{}`", entry.key);
        let diagnostic = Diagnostic::new(Code::V1004, entry.key_span, message)
            .label("not an error key")
            .help("an expected error writes `message`, `path`, `code` (its `extensions.code`) or `extensions`");
        cx.push(suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: &KEYS }));
        return None;
    }
    if let (true, Value::Seq(items)) = (entry.key == "path", &entry.value.value) {
        cx.push(listed_path(&entry.value, items));
        return None;
    }
    Some((entry.key.clone(), super::matcher(cx, at.child(&entry.value), Unit::Plain)?))
}

/// V2306 for a `path` written as a list: errors are compared with it dotted, so a list never
/// matches. When each segment is text or a whole number, the dotted text is the certain fix.
fn listed_path(node: &Node, items: &[Node]) -> Diagnostic {
    let diagnostic = Diagnostic::new(Code::V2306, node.span, "an expected error's `path` is dotted, not a list")
        .label("never matches")
        .help("errors are compared with `path` dotted as body paths are: `user.notes[0]`");
    let Some(segments) = items.iter().map(segment).collect::<Option<Vec<_>>>() else { return diagnostic };
    let text = vc_template::dotted(&segments);
    let edit = TextEdit::replace(node.span, format!("'{}'", text.replace('\'', "''")));
    diagnostic.fix(Fix::certain(format!("Write the path as `{text}`"), vec![edit]))
}

/// A literal path segment: a whole number is an index and text a key; `null`, a boolean, a
/// decimal or a template is none, since what the error's path holds there is not certain.
fn segment(item: &Node) -> Option<Segment> {
    let scalar = item.as_scalar().filter(|scalar| !scalar.text.contains("${"))?;
    match scalar.kind {
        ScalarKind::Int => scalar.text.parse().ok().map(Segment::Index),
        ScalarKind::String => Some(Segment::Key(scalar.text.clone())),
        ScalarKind::Null | ScalarKind::Bool | ScalarKind::Float => None,
    }
}

fn call((name, arg): (&str, MatcherArg), span: SourceSpan) -> MatcherCall {
    MatcherCall { name: name.to_owned(), arg, span }
}

fn one(call: MatcherCall) -> Matcher {
    Matcher::All(vec![call])
}
