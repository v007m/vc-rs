//! V3205: a test-level `body` default reaches a `GET` or `HEAD` step (D124).

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Method, SourceSpan};
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link::{list, method, text};

/// The methods that send no body.
const READS: [Method; 2] = [Method::Get, Method::Head];

/// Reports every read step a test's `body` reaches.
pub struct BodyOnRead;

impl Lint for BodyOnRead {
    fn code(&self) -> Code {
        Code::V3205
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        for (source, test) in cx.tests() {
            let Some(body) = test.entry("body") else { continue };
            let bare = list(test, "steps").iter().filter(|step| step.get("body").is_none());
            let reads = bare.filter(|step| cx.api(source, step).is_some_and(|api| READS.contains(&method(api))));
            found.extend(reads.filter_map(|step| self.reaches(step, body.key_span)));
        }
        found
    }
}

impl BodyOnRead {
    fn reaches(&self, step: &Node, body: SourceSpan) -> Option<Diagnostic> {
        let call = step.entry("call")?;
        let name = text(step, "call").unwrap_or_default();
        let message = format!("the test's `body` is sent with `{name}`, a call that takes no body");
        let diagnostic = Diagnostic::new(self.code(), call.value.span, message).label("a GET or HEAD call");
        let diagnostic = diagnostic.secondary(body, "the test-level default");
        Some(diagnostic.help("move `body` onto the steps that send one"))
    }
}
