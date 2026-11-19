//! What `expect.errors` expects of a `graphql` response's errors (D345), as matchers on the
//! normalised list: a list of entries, where each entry matches some error and each error
//! matches some entry, or matchers whose literal maps are entries (`{ some: { code: TIMEOUT } }`).
//! An entry lowers to the internal `fields` matcher, a literal or matcher per key it writes (D360).

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Matcher, MatcherArg, MatcherCall, SourceSpan};
use vc_parser::{Entry, Node, Value};

use super::{Unit, calls, find, literal};
use crate::parts::{At, Cx};
use crate::suggest::{self, Misspelling};

/// The keys an entry may write: `code` is short for `extensions.code`.
const KEYS: [&str; 4] = ["message", "path", "code", "extensions"];

/// The matcher `at` holds under `expect.errors`.
pub fn expected(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    match &at.node.value {
        Value::Seq(items) => listed(cx, at, items),
        Value::Map(entries) if entries.iter().any(|entry| find(&entry.key).is_some()) => {
            calls(cx, at, (entries, Unit::Errors))
        }
        Value::Map(entries) => entry(cx, at, entries),
        Value::Scalar(_) => literal(cx, at, Unit::Plain),
    }
}

/// The list form: `allOf: [every: {anyOf: [e1, e2]}, some: e1, some: e2]`.
fn listed(cx: &mut Cx<'_>, at: At<'_>, items: &[Node]) -> Option<Matcher> {
    let found: Vec<_> = items.iter().map(|item| item_entry(cx, at.child(item))).collect();
    let entries = found.into_iter().collect::<Option<Vec<_>>>()?;
    let span = at.node.span;
    let any = one(call(("anyOf", MatcherArg::Matchers(entries.clone())), span));
    let every = one(call(("every", MatcherArg::Matcher(Box::new(any))), span));
    let some = entries.into_iter().map(|entry| one(call(("some", MatcherArg::Matcher(Box::new(entry))), span)));
    Some(one(call(("allOf", MatcherArg::Matchers(std::iter::once(every).chain(some).collect())), span)))
}

/// One item of the list form, which must be an entry.
fn item_entry(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    if let Value::Map(entries) = &at.node.value {
        return entry(cx, at, entries);
    }
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
    Some((entry.key.clone(), super::matcher(cx, at.child(&entry.value), Unit::Plain)?))
}

fn call((name, arg): (&str, MatcherArg), span: SourceSpan) -> MatcherCall {
    MatcherCall { name: name.to_owned(), arg, span }
}

fn one(call: MatcherCall) -> Matcher {
    Matcher::All(vec![call])
}
