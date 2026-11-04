//! V3202: `cache` on an action that runs for every test, where a cached result is reused by
//! tests that expect their own.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link;
use crate::link::TEST_BLOCKS;

/// The test file's blocks that run once per test.
const PER_TEST_BLOCKS: [&str; 2] = ["beforeEach", "afterEach"];

/// Reports every `cache` on a per-test action.
pub struct TestCache;

impl Lint for TestCache {
    fn code(&self) -> Code {
        Code::V3202
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let blocks =
            cx.test_files().flat_map(|source| PER_TEST_BLOCKS.iter().flat_map(|key| link::list(source.root, key)));
        let steps = cx.tests().flat_map(|(_, test)| TEST_BLOCKS.iter().flat_map(|key| link::list(test, key)));
        blocks.chain(steps).filter_map(|action| self.cached(action)).collect()
    }
}

impl TestCache {
    fn cached(&self, action: &Node) -> Option<Diagnostic> {
        let entry = action.entry("cache")?;
        let diagnostic = Diagnostic::new(self.code(), entry.key_span, "`cache` on an action that runs for every test");
        let diagnostic = diagnostic.label("cached per test");
        Some(diagnostic.help("`cache` reuses one result across runs; put the action in `beforeAll` or `beforeRun`"))
    }
}
