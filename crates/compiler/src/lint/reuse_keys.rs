//! V3207: a request or check key on a `test:` action, which runs the reused test as written
//! (D270), so the key has no effect.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_parser::Entry;

use super::{Lint, LintCx, whole_lines};
use crate::link;
use crate::link::Source;

/// The keys a `test:` action ignores.
const IGNORED: [&str; 6] = ["auth", "headers", "query", "body", "poll", "expect"];

/// Reports every key a `test:` action ignores.
pub struct ReuseKeys;

impl Lint for ReuseKeys {
    fn code(&self) -> Code {
        Code::V3207
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        for source in &cx.project.sources {
            let reuses = link::actions(source).into_iter().filter(|action| action.get("test").is_some());
            let entries = reuses.flat_map(|action| IGNORED.iter().filter_map(|key| action.entry(key)));
            found.extend(entries.map(|entry| self.ignored(source, entry)));
        }
        found
    }
}

impl ReuseKeys {
    fn ignored(&self, source: &Source<'_>, entry: &Entry) -> Diagnostic {
        let key = entry.key.as_str();
        let message = format!("a `test:` action ignores `{key}`");
        let diagnostic = Diagnostic::new(self.code(), entry.key_span, message)
            .label("has no effect")
            .note("a `test:` action runs the reused test as written, with its own requests and checks (D270)")
            .help(format!("remove `{key}`, or set it in the reused test"));
        let Some(range) = whole_lines(source.text, entry) else { return diagnostic };
        let edit = TextEdit { file: source.file, range, new_text: String::new() };
        diagnostic.fix(Fix::certain(format!("Remove `{key}`"), vec![edit]))
    }
}
