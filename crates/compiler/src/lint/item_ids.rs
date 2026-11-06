//! V4001 and V4002: a test or api ID that is not camelCase, with the rename (and every
//! reference to it) as the fix.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::FileKind;
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

/// The warning for an item whose ID is not camelCase; `name` is its `name:`, if any.
fn badly_named(cx: &LintCx<'_, '_>, code: Code, (source, item, name): (&Source<'_>, &Node, Option<&str>)) -> Option<Diagnostic> {
    let node = item.get("id")?;
    let id = node.as_scalar()?.text.as_str();
    if naming::is_camel_id(id) {
        return None;
    }
    let from_name = name.map(naming::camel_id).filter(|derived| naming::is_camel_id(derived));
    let better = from_name.unwrap_or_else(|| naming::camel_id(id));
    let what = if code == Code::V4001 { "test" } else { "api" };
    let diagnostic = Diagnostic::new(code, node.span, format!("{what} ID `{id}` is not camelCase")).label("rename it");
    if !naming::is_camel_id(&better) {
        return Some(diagnostic.help(format!("{what} IDs are camelCase letters and digits")));
    }
    let diagnostic = diagnostic.help(format!("{what} IDs are camelCase: `{better}`"));
    let (from, to) = (source.full(id), source.full(&better));
    if cx.project.tests.contains_key(&to) || cx.project.apis.contains_key(&to) {
        return Some(diagnostic);
    }
    let mut edits = vec![respelled(source, node, &better)];
    edits.extend(reference_edits(cx, &from, &to));
    Some(diagnostic.fix(Fix::certain(format!("Rename to {better}"), edits)))
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
