//! V3103: a `skip` on a test file or a test that gives no reason.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link::list;

/// Reports every `skip` whose reason is blank.
pub struct SkipReasons;

impl Lint for SkipReasons {
    fn code(&self) -> Code {
        Code::V3103
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let files = cx.test_files().map(|source| source.root);
        let skippable = files.flat_map(|root| std::iter::once(root).chain(list(root, "tests")));
        skippable.filter_map(|node| node.get("skip")).filter_map(|skip| self.blank(skip)).collect()
    }
}

impl SkipReasons {
    fn blank(&self, skip: &Node) -> Option<Diagnostic> {
        let reason = skip.get("reason").unwrap_or(skip);
        let text = reason.as_scalar().map_or("", |scalar| scalar.text.as_str());
        if !text.trim().is_empty() {
            return None;
        }
        let diagnostic = Diagnostic::new(self.code(), reason.span, "`skip` needs a reason").label("no reason given");
        Some(diagnostic.help("say why, for the report: `skip: flaky upstream, see #123`"))
    }
}
