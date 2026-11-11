//! V4001 and V4002: a test ID that is not a camelCase third-person verb, or an api ID that is
//! not camelCase, with the rename (and every reference to it) as the fix.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::{FileKind, is_third_person, third_person};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link::{self, Source};
use crate::naming;

/// V4001: test IDs, renamed from the test's name when it gives a good one.
pub struct TestIds;

/// V4002: api IDs.
pub struct ApiIds;

impl Lint for TestIds {
    fn code(&self) -> Code {
        Code::V4001
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let tests = cx.tests().map(|(source, test)| (source, test, link::text(test, "name")));
        tests.filter_map(|item| badly_named(cx, self.code(), item)).collect()
    }
}

impl Lint for ApiIds {
    fn code(&self) -> Code {
        Code::V4002
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let files = cx.project.sources.iter().filter(|source| source.kind == FileKind::Api);
        let apis = files.flat_map(|source| link::list(source.root, "apis").iter().map(move |api| (source, api, None)));
        apis.filter_map(|item| badly_named(cx, self.code(), item)).collect()
    }
}

/// The warning for an item whose ID breaks the convention; `name` is its `name:`, if any.
fn badly_named(
    cx: &LintCx<'_, '_>,
    code: Code,
    (source, item, name): (&Source<'_>, &Node, Option<&str>),
) -> Option<Diagnostic> {
    let node = item.get("id")?;
    let id = node.as_scalar()?.text.as_str();
    let fault = fault(code, id)?;
    let what = if code == Code::V4001 { "test" } else { "api" };
    let diagnostic = Diagnostic::new(code, node.span, format!("{what} ID `{id}` {fault}")).label("rename it");
    let (better, certain) = better(code, id, name);
    if !naming::is_camel_id(&better) {
        return Some(diagnostic.help(format!("{what} IDs are camelCase letters and digits")));
    }
    let rule = if code == Code::V4001 { "test IDs are camelCase third-person verbs" } else { "api IDs are camelCase" };
    let diagnostic = diagnostic.help(format!("{rule}: `{better}`"));
    let (from, to) = (source.full(id), source.full(&better));
    if cx.project.tests.contains_key(&to) || cx.project.apis.contains_key(&to) {
        return Some(diagnostic);
    }
    let mut edits = vec![respelled(source, node, &better)];
    edits.extend(reference_edits(cx, &from, &to));
    let title = format!("Rename to {better}");
    Some(diagnostic.fix(if certain { Fix::certain(title, edits) } else { Fix::suggested(title, edits) }))
}

/// What breaks the convention in `id`, for the message; `None` when nothing does. A test ID
/// starts with a third-person verb as well (`getsUser`, not `getUser` or `userTest`).
fn fault(code: Code, id: &str) -> Option<&'static str> {
    if !naming::is_camel_id(id) {
        return Some("is not camelCase");
    }
    (code == Code::V4001 && !is_third_person(id)).then_some("does not start with a third-person verb")
}

/// The ID to rename `id` to, and whether the rename is certain: the item's `name` as an ID when
/// that keeps the convention, else `id` in camelCase. A test ID whose first word is then not a
/// third-person verb has it conjugated (`getUser` gives `getsUser`), which is only a guess.
fn better(code: Code, id: &str, name: Option<&str>) -> (String, bool) {
    let from_name = name.map(naming::camel_id).filter(|derived| fault(code, derived).is_none());
    if let Some(from_name) = from_name {
        return (from_name, true);
    }
    let camel = naming::camel_id(id);
    if camel.is_empty() || fault(code, &camel).is_none() {
        return (camel, true);
    }
    (third_person(&camel), false)
}

/// The edits that point every reference to the full ID `from` at `to` instead, keeping a
/// reference short when it was short and stays in its file.
fn reference_edits(cx: &LintCx<'_, '_>, from: &str, to: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    for source in &cx.project.sources {
        for node in vc_parser::references(source.root) {
            let Some(written) = node.as_str() else { continue };
            if source.resolve(written) != from {
                continue;
            }
            let short = to.strip_prefix(&source.full("")).filter(|_| !written.contains('#'));
            edits.push(respelled(source, node, short.unwrap_or(to)));
        }
    }
    edits
}

/// Replaces the scalar `node` with `text`, in the same quotes.
fn respelled(source: &Source<'_>, node: &Node, text: &str) -> TextEdit {
    let written = source.text.get(node.span.byte_range()).unwrap_or_default();
    let quote = written.chars().next().filter(|c| matches!(c, '\'' | '"'));
    TextEdit::replace(node.span, quote.map_or_else(|| text.to_owned(), |quote| format!("{quote}{text}{quote}")))
}
