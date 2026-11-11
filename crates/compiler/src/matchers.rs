//! Matchers (see Checks and matchers): a map with a matcher name as a key is a matcher, any
//! other value is a literal that must be equal. Each matcher's argument is checked here, and a
//! wrong one is V2306; an unknown key next to a matcher is V1004.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Matcher, MatcherArg, MatcherCall, SourceSpan, TemplateValue};
use vc_parser::{Entry, Node, ScalarKind, Value};

use crate::parts::{At, Cx};
use crate::shapes::Unit;
use crate::suggest::{self, Misspelling};
use crate::value;

/// What a matcher takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arg {
    /// Any value: `contains`.
    Value,
    /// A value compared in the unit: `eq`, `gt`, `lt`.
    Ordered,
    /// Two values compared in the unit: `between`.
    Range,
    /// A list of values: `in`.
    List,
    /// One of a list of words: `type`, `format`.
    Word(&'static [&'static str]),
    /// `true` or `false`: `exists`, `empty`.
    Flag,
    /// A number or a matcher: `length`.
    Length,
    /// A regular expression.
    Regex,
    /// A matcher: `every`, `not`.
    Nested,
    /// A list of matchers: `allOf`, `anyOf`.
    Several,
}

const TYPES: &[&str] = &["string", "number", "integer", "boolean", "object", "array", "null"];
const FORMATS: &[&str] = &["email", "uuid", "uri", "date", "dateTime", "ipv4"];

/// Every matcher and what it takes.
const MATCHERS: &[(&str, Arg)] = &[
    ("eq", Arg::Ordered),
    ("ne", Arg::Ordered),
    ("gt", Arg::Ordered),
    ("gte", Arg::Ordered),
    ("lt", Arg::Ordered),
    ("lte", Arg::Ordered),
    ("between", Arg::Range),
    ("in", Arg::List),
    ("notIn", Arg::List),
    ("type", Arg::Word(TYPES)),
    ("exists", Arg::Flag),
    ("empty", Arg::Flag),
    ("length", Arg::Length),
    ("contains", Arg::Value),
    ("startsWith", Arg::Value),
    ("endsWith", Arg::Value),
    ("regex", Arg::Regex),
    ("format", Arg::Word(FORMATS)),
    ("every", Arg::Nested),
    ("some", Arg::Nested),
    ("none", Arg::Nested),
    ("not", Arg::Nested),
    ("allOf", Arg::Several),
    ("anyOf", Arg::Several),
];

fn find(name: &str) -> Option<Arg> {
    MATCHERS.iter().find(|(known, _)| *known == name).map(|(_, arg)| *arg)
}

/// A matcher's argument, where it is written and the unit it compares in.
#[derive(Clone, Copy)]
struct Spot<'n> {
    at: At<'n>,
    unit: Unit,
    name: &'n str,
}

/// The matcher or literal at `at`, comparing in `unit`.
pub fn matcher(cx: &mut Cx<'_>, at: At<'_>, unit: Unit) -> Option<Matcher> {
    let Value::Map(entries) = &at.node.value else { return literal(cx, at, unit) };
    if !entries.iter().any(|entry| find(&entry.key).is_some()) && !misspelt(entries) {
        return value::lower(cx, at).map(Matcher::Equals);
    }
    calls(cx, at, (entries, unit, Literal::Allowed))
}

/// A scalar literal: under a unit (`size: 32b`, `total: 200ms`) it is read in that unit and
/// compared as `eq`, so it meets the measured number; anywhere else it is compared as written.
fn literal(cx: &mut Cx<'_>, at: At<'_>, unit: Unit) -> Option<Matcher> {
    if unit == Unit::Plain || at.node.as_scalar().is_none() {
        return value::lower(cx, at).map(Matcher::Equals);
    }
    let arg = ordered(cx, Spot { at, unit, name: "eq" })?;
    Some(Matcher::All(vec![MatcherCall { name: "eq".to_owned(), arg, span: at.node.span }]))
}

/// A status's matcher: a map there is never a literal, so each of its keys must be a matcher.
pub fn status(cx: &mut Cx<'_>, at: At<'_>) -> Option<Matcher> {
    let Value::Map(entries) = &at.node.value else { return matcher(cx, at, Unit::Plain) };
    calls(cx, at, (entries, Unit::Plain, Literal::Refused))
}

/// Whether a map at the spot could have been a literal object, which the V1004 help then explains.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Literal {
    Allowed,
    Refused,
}

fn calls(cx: &mut Cx<'_>, at: At<'_>, (entries, unit, literal): (&[Entry], Unit, Literal)) -> Option<Matcher> {
    let alone = literal == Literal::Allowed && entries.len() == 1;
    let calls: Vec<_> = entries.iter().map(|entry| call(cx, at, (entry, unit, alone))).collect();
    calls.into_iter().collect::<Option<Vec<_>>>().map(Matcher::All)
}

/// The shortest lone key read as a misspelt matcher: shorter ones (`id`, `note`) are one edit
/// from a matcher (`in`, `not`) and are far more often plain fields.
const MISSPELT_FROM: usize = 5;

/// How to keep a lone key read as a misspelt matcher as a literal object.
const LITERAL_HELP: &str = "to compare with an object that has this key, put the object under `eq:`";

/// Whether `entries` is one key close to a matcher's name (`startWith`, `lenght`), which is
/// far more likely a misspelt matcher than a literal object.
fn misspelt(entries: &[Entry]) -> bool {
    let [entry] = entries else { return false };
    entry.key.chars().count() >= MISSPELT_FROM && !suggest::close(&entry.key, names()).is_empty()
}

/// The matchers that compare one value, which the list a `[*]` path yields never is.
const SCALAR: &[&str] = &["gt", "gte", "lt", "lte", "between", "startsWith", "endsWith", "regex", "format"];

/// Reports each scalar matcher in `matcher`, written at the `[*]` path `key`, as V2306. `not`,
/// `allOf` and `anyOf` still see the list; `every`, `some` and `none` see each element.
pub fn on_list(cx: &mut Cx<'_>, matcher: &Matcher, key: &str) {
    let Matcher::All(calls) = matcher else { return };
    for call in calls {
        match &call.arg {
            _ if SCALAR.contains(&call.name.as_str()) => cx.push(scalar_on_list(call, key)),
            MatcherArg::Matcher(inner) if call.name == "not" => on_list(cx, inner, key),
            MatcherArg::Matchers(all) => all.iter().for_each(|inner| on_list(cx, inner, key)),
            _ => {}
        }
    }
}

fn scalar_on_list(call: &MatcherCall, key: &str) -> Diagnostic {
    let message = format!("`{}` compares one value, but `{key}` is a list", call.name);
    let help = format!("to check each element, put it under `every:`, as in `{{every: {{{}: …}}}}`", call.name);
    Diagnostic::new(Code::V2306, call.span, message).label("not a list matcher").help(help)
}

fn names() -> impl Iterator<Item = &'static str> {
    MATCHERS.iter().map(|(name, _)| *name)
}

fn call(cx: &mut Cx<'_>, at: At<'_>, (entry, unit, alone): (&Entry, Unit, bool)) -> Option<MatcherCall> {
    let Some(kind) = find(&entry.key) else {
        let message = format!("unknown matcher `{}`", entry.key);
        let diagnostic = Diagnostic::new(Code::V1004, entry.key_span, message).label("not a matcher");
        let names: Vec<&str> = names().collect();
        let hinted = suggest::hint(diagnostic, &Misspelling { word: &entry.key, span: entry.key_span, known: &names });
        cx.push(if alone { hinted.help(LITERAL_HELP) } else { hinted });
        return None;
    };
    let spot = Spot { at: at.child(&entry.value), unit, name: &entry.key };
    let arg = argument(cx, spot, kind)?;
    Some(MatcherCall {
        name: entry.key.clone(),
        arg,
        span: SourceSpan::new(entry.key_span.file, entry.key_span.start, entry.value.span.end),
    })
}

fn argument(cx: &mut Cx<'_>, spot: Spot<'_>, kind: Arg) -> Option<MatcherArg> {
    match kind {
        Arg::Value => value::lower(cx, spot.at).map(MatcherArg::Value),
        Arg::Ordered => ordered(cx, spot),
        Arg::Range => range(cx, spot),
        Arg::List => list(cx, spot),
        Arg::Word(words) => word(cx, spot, words),
        Arg::Flag => flag(cx, spot),
        Arg::Length => length(cx, spot),
        Arg::Regex => regex(cx, spot),
        Arg::Nested => matcher(cx, spot.at, spot.unit).map(|m| MatcherArg::Matcher(Box::new(m))),
        Arg::Several => several(cx, spot),
    }
}

/// A value compared in the spot's unit; a template is compared as written.
fn ordered(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    let node = spot.at.node;
    let text = node.as_scalar().map(|s| s.text.as_str()).filter(|text| !text.contains("${"));
    match (spot.unit, text) {
        (Unit::Duration, Some(text)) => {
            cx.check(vc_template::parse_duration(text, node.span)).map(MatcherArg::Duration)
        }
        (Unit::Size, Some(text)) => cx.check(vc_template::parse_size(text, node.span)).map(MatcherArg::Size),
        (Unit::Rate, Some(text)) => cx
            .check(vc_template::parse_rate(text, node.span))
            .map(|share| MatcherArg::Value(TemplateValue::Number(share))),
        _ => value::lower(cx, spot.at).map(MatcherArg::Value),
    }
}

fn range(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    let Value::Seq(items) = &spot.at.node.value else { return invalid(cx, spot, "needs a list of two values") };
    let [low, high] = items.as_slice() else { return invalid(cx, spot, "needs exactly two values, low and high") };
    let low = ordered(cx, Spot { at: spot.at.child(low), ..spot });
    let high = ordered(cx, Spot { at: spot.at.child(high), ..spot });
    Some(MatcherArg::Range(Box::new([low?, high?])))
}

fn list(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    if !matches!(spot.at.node.value, Value::Seq(_)) {
        return invalid(cx, spot, "needs a list of values");
    }
    value::lower(cx, spot.at).map(MatcherArg::Value)
}

fn word(cx: &mut Cx<'_>, spot: Spot<'_>, words: &'static [&'static str]) -> Option<MatcherArg> {
    let node = spot.at.node;
    let Some(text) = node.as_str() else { return invalid(cx, spot, &format!("needs one of {}", words.join(", "))) };
    if !words.contains(&text) {
        let message = format!("`{}` does not know `{text}`", spot.name);
        let diagnostic = Diagnostic::new(Code::V2306, node.span, message);
        cx.push(suggest::hint(diagnostic, &Misspelling { word: text, span: node.span, known: words }));
        return None;
    }
    Some(MatcherArg::Value(TemplateValue::String(vc_model::Template {
        parts: vec![vc_model::Part::Literal(text.to_owned())],
    })))
}

fn flag(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    let flag = spot.at.node.as_scalar().filter(|s| s.kind == ScalarKind::Bool);
    let Some(flag) = flag else { return invalid(cx, spot, "takes `true` or `false`") };
    Some(MatcherArg::Value(TemplateValue::Bool(flag.text.eq_ignore_ascii_case("true"))))
}

fn length(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    match &spot.at.node.value {
        Value::Map(_) => matcher(cx, spot.at, Unit::Plain).map(|m| MatcherArg::Matcher(Box::new(m))),
        Value::Scalar(s) if s.kind == ScalarKind::Int || s.text.contains("${") => {
            value::lower(cx, spot.at).map(MatcherArg::Value)
        }
        _ => invalid(cx, spot, "takes a whole number or a matcher"),
    }
}

fn regex(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    let Some(pattern) = spot.at.node.as_str() else { return invalid(cx, spot, "takes a pattern") };
    if !pattern.contains("${")
        && let Err(error) = regex::Regex::new(pattern)
    {
        let reason = error.to_string().lines().last().unwrap_or_default().trim().to_owned();
        return invalid(cx, spot, &format!("has an invalid pattern: {reason}"));
    }
    value::lower(cx, spot.at).map(MatcherArg::Value)
}

fn several(cx: &mut Cx<'_>, spot: Spot<'_>) -> Option<MatcherArg> {
    let Value::Seq(items) = &spot.at.node.value else { return invalid(cx, spot, "needs a list of matchers") };
    let found: Vec<_> = items.iter().map(|item| matcher(cx, spot.at.child(item), spot.unit)).collect();
    found.into_iter().collect::<Option<Vec<_>>>().map(MatcherArg::Matchers)
}

/// Records V2306 at the argument and gives no argument.
fn invalid(cx: &mut Cx<'_>, spot: Spot<'_>, problem: &str) -> Option<MatcherArg> {
    let node: &Node = spot.at.node;
    cx.push(Diagnostic::new(Code::V2306, node.span, format!("`{}` {problem}", spot.name)));
    None
}

#[cfg(test)]
mod tests {
    use super::{Arg, MATCHERS, find};

    #[test]
    fn knows_every_matcher_of_the_syntax_reference_once() {
        assert_eq!(MATCHERS.len(), 24);
        let mut names: Vec<_> = MATCHERS.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), MATCHERS.len());
        assert_eq!(find("between"), Some(Arg::Range));
        assert_eq!(find("betwen"), None);
    }
}
