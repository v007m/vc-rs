//! V3103: a `skip` on a test file or a test that gives no reason: a blank one, or `true` or
//! `false` in its place.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Node, ScalarKind};

use super::{Lint, LintCx};
use crate::link;

/// What the help suggests as a reason: no ` #`, which YAML would read as a comment.
const EXAMPLE: &str = "`skip: flaky upstream, see issue 123`";

/// Reports every `skip` whose reason is blank or a boolean.
pub struct SkipReasons;

impl Lint for SkipReasons {
    fn code(&self) -> Code {
        Code::V3103
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let files = cx.test_files().map(|source| source.root);
        let skippable = files.flat_map(|root| std::iter::once(root).chain(link::list(root, "tests")));
        skippable.filter_map(|node| node.get("skip")).filter_map(|skip| self.blank(skip)).collect()
    }
}

impl SkipReasons {
    fn blank(&self, skip: &Node) -> Option<Diagnostic> {
        let reason = skip.get("reason").unwrap_or(skip);
        let scalar = reason.as_scalar();
        let text = scalar.map_or("", |scalar| scalar.text.as_str());
        let boolean = scalar.is_some_and(|scalar| scalar.kind == ScalarKind::Bool);
        if !boolean && !text.trim().is_empty() {
            return None;
        }
        let diagnostic = Diagnostic::new(self.code(), reason.span, "`skip` needs a reason");
        if !boolean {
            return Some(diagnostic.label("no reason given").help(format!("say why, for the report: {EXAMPLE}")));
        }
        let help = format!("`skip` takes the reason itself, not `{text}`: {EXAMPLE}; to run the test, delete `skip`");
        Some(diagnostic.label(format!("`{text}` is not a reason")).help(help))
    }
}
