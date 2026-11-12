//! A dated setting on a test file or a test whose `until` day has passed, so it no longer holds:
//! a `skip`, and the test runs again (V3104), or an `allowFailure`, and a failure fails the run again
//! (V4008, D248).

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::Date;
use vc_parser::Entry;

use super::{Lint, LintCx, whole_lines};
use crate::link;
use crate::link::Source;

/// Reports every `key` whose `until` is before today; needs today's date, so it finds nothing
/// when the caller gave none.
pub struct Expired {
    /// The key: `skip` or `allowFailure`.
    key: &'static str,
    /// The code it reports.
    code: Code,
    /// What no longer holds after `until`.
    note: &'static str,
    /// What to do instead.
    help: &'static str,
}

/// V3104: an expired `skip`.
pub const SKIPS: Expired = Expired {
    key: "skip",
    code: Code::V3104,
    note: "an expired skip is ignored, so the test runs again",
    help: "remove the skip, or move `until` later if the reason still holds",
};

/// V4008: an expired `allowFailure` (D248).
pub const ALLOWED_FAILURES: Expired = Expired {
    key: "allowFailure",
    code: Code::V4008,
    note: "an expired allowFailure is ignored, so a failure fails the run again",
    help: "remove the allowFailure, or move `until` later if the reason still holds",
};

impl Lint for Expired {
    fn code(&self) -> Code {
        self.code
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(today) = cx.today else { return Vec::new() };
        let mut found = Vec::new();
        for source in cx.test_files() {
            let dated = std::iter::once(source.root).chain(link::list(source.root, "tests"));
            found.extend(dated.filter_map(|node| self.expired(source, node.entry(self.key)?, today)));
        }
        found
    }
}

impl Expired {
    fn expired(&self, source: &Source<'_>, entry: &Entry, today: Date) -> Option<Diagnostic> {
        let until = entry.value.get("until")?;
        let last = Date::parse(until.as_scalar()?.text.as_str())?;
        if today <= last {
            return None;
        }
        let message = format!("this {} expired after {last}", self.key);
        let diagnostic = Diagnostic::new(self.code, until.span, message).label(format!("today is {today}"));
        let diagnostic = diagnostic.note(self.note).help(self.help);
        let Some(range) = whole_lines(source.text, entry) else { return Some(diagnostic) };
        let edit = TextEdit { file: source.file, range, new_text: String::new() };
        Some(diagnostic.fix(Fix::certain(format!("Remove the {}", self.key), vec![edit])))
    }
}
