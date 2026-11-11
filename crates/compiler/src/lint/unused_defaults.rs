//! V3206: a test-level default that no step uses (D124), because every `call` step sets its own,
//! or the test has none (`test:` steps take no defaults, D270).

use std::collections::HashMap;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Action, Check, Origin, Program, SourceSpan, Test, TestBody};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link;
use crate::link::Source;

/// The defaults a step's own value replaces.
const REPLACED: [&str; 3] = ["auth", "body", "poll"];

/// The defaults a step's own map merges over, name by name.
const MERGED: [&str; 2] = ["headers", "query"];

/// Reports every test-level default of a test with steps that reaches no step.
pub struct UnusedDefaults;

impl Lint for UnusedDefaults {
    fn code(&self) -> Code {
        Code::V3206
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let built = Built::new(cx.program);
        let mut found = Vec::new();
        for (source, test) in cx.tests().filter(|(_, test)| test.get("steps").is_some()) {
            let takers: Vec<&Node> = link::list(test, "steps").iter().filter(|step| takes(step)).collect();
            let mut unused = replaced(test, &takers);
            unused.extend(merged(test, &takers));
            unused.extend(built.checks(source, test));
            found.extend(unused.into_iter().map(|(span, what)| self.report(span, &what)));
        }
        found
    }
}

impl UnusedDefaults {
    fn report(&self, span: SourceSpan, what: &str) -> Diagnostic {
        let message = format!("no step uses the test-level `{what}`");
        Diagnostic::new(self.code(), span, message)
            .label("unused default")
            .note("`call` steps take the test's defaults and their own values win; `test:`, `function` and `command` steps take none")
            .help("remove it, or remove the steps' own values that replace it")
    }
}

/// Whether a step takes the test's defaults: only a `call` does (D270).
fn takes(step: &Node) -> bool {
    step.get("call").is_some()
}

/// The replaced defaults every taker sets itself.
fn replaced(test: &Node, takers: &[&Node]) -> Vec<(SourceSpan, String)> {
    let written = REPLACED.iter().filter_map(|key| Some((*key, test.entry(key)?)));
    let unused = written.filter(|(key, _)| takers.iter().all(|step| step.get(key).is_some()));
    unused.map(|(key, entry)| (entry.key_span, key.to_owned())).collect()
}

/// The merged names every taker sets itself.
fn merged(test: &Node, takers: &[&Node]) -> Vec<(SourceSpan, String)> {
    let mut unused = Vec::new();
    for key in MERGED {
        let entries = test.get(key).map(Node::entries).unwrap_or_default();
        let own = |name: &str| takers.iter().all(|step| step.get(key).and_then(|map| map.get(name)).is_some());
        let names = entries.iter().filter(|entry| own(&entry.key));
        unused.extend(names.map(|entry| (entry.key_span, format!("{key}.{}", entry.key))));
    }
    unused
}

/// The program's tests by full ID, and its actions.
struct Built<'p> {
    tests: HashMap<&'p str, &'p Test>,
    actions: &'p [Action],
}

impl<'p> Built<'p> {
    fn new(program: &'p Program) -> Self {
        let tests = program.tests.values().filter_map(|test| Some((program.str(test.id.sym())?, test)));
        Self { tests: tests.collect(), actions: &program.actions }
    }

    /// The test-level checks of `test` that no step's merged checks kept.
    fn checks(&self, source: &Source<'_>, test: &Node) -> Vec<(SourceSpan, String)> {
        let Some(built) = link::text(test, "id").and_then(|id| self.tests.get(source.full(id).as_str())) else {
            return Vec::new();
        };
        let TestBody::Steps(ids) = &built.body else { return Vec::new() };
        let steps: Vec<&Action> =
            ids.iter().filter_map(|id| self.actions.get(usize::try_from(id.index()).ok()?)).collect();
        let kept = |span: SourceSpan| {
            let from_test = |check: &Check| check.origin == Origin::Test(built.id) && check.span == span;
            steps.iter().any(|step| step.expect.checks.iter().any(from_test))
        };
        let unused = built.expect.checks.iter().filter(|check| !kept(check.span));
        unused.map(|check| (check.span, "expect".to_owned())).collect()
    }
}
