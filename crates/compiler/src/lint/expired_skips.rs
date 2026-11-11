//! V3104: a `skip` on a test file or a test whose `until` day has passed, so it runs again.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::Date;
use vc_parser::Entry;

use super::{Lint, LintCx, whole_lines};
use crate::link;
use crate::link::Source;

/// Reports every `skip` whose `until` is before today; needs today's date, so it finds nothing
/// when the caller gave none.
pub struct ExpiredSkips;

impl Lint for ExpiredSkips {
    fn code(&self) -> Code {
        Code::V3104
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(today) = cx.today else { return Vec::new() };
        let mut found = Vec::new();
        for source in cx.test_files() {
            let skippable = std::iter::once(source.root).chain(link::list(source.root, "tests"));
            found.extend(skippable.filter_map(|node| self.expired(source, node.entry("skip")?, today)));
        }
        found
    }
}

impl ExpiredSkips {
    fn expired(&self, source: &Source<'_>, skip: &Entry, today: Date) -> Option<Diagnostic> {
        let until = skip.value.get("until")?;
        let last = Date::parse(until.as_scalar()?.text.as_str())?;
        if today <= last {
            return None;
        }
        let message = format!("this skip expired after {last}");
        let diagnostic = Diagnostic::new(self.code(), until.span, message).label(format!("today is {today}"));
        let diagnostic = diagnostic
            .note("an expired skip is ignored, so the test runs again")
            .help("remove the skip, or move `until` later if the reason still holds");
        let Some(range) = whole_lines(source.text, skip) else { return Some(diagnostic) };
        let edit = TextEdit { file: source.file, range, new_text: String::new() };
        Some(diagnostic.fix(Fix::certain("Remove the skip", vec![edit])))
    }
}
