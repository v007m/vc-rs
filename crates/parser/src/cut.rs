//! V3204: a plain value that YAML cut short at ` #`, reading the rest of the line as a comment.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::{Pos, SourceSpan};

use crate::scalar::double_quote;

/// A warning when the plain text scalar at `at` is followed on its line by a comment that looks
/// like the rest of the value: `see #212`, or a value ending in a comma. The caller passes only
/// text-kind scalars, so typed values (`retries: 3 #max`) are never rewritten; the fix is a
/// suggestion because the `#` may really start a comment.
pub fn check(text: &str, at: SourceSpan) -> Option<Diagnostic> {
    let range = at.byte_range();
    if range.is_empty() || at.start.line != at.end.line {
        return None;
    }
    let value = text.get(range.clone())?;
    let rest = text.get(range.end..)?;
    let line = rest.split('\n').next().unwrap_or_default().trim_end();
    let comment = line.trim_start_matches([' ', '\t']);
    if comment.len() == line.len() || !comment.starts_with('#') || !looks_cut(value, comment) {
        return None;
    }
    let end = range.end + line.len();
    let whole = text.get(range.start..end)?;
    let columns = u32::try_from(line.chars().count()).unwrap_or(u32::MAX);
    let span = SourceSpan::new(at.file, at.start, Pos::new(at.end.line, at.end.column.saturating_add(columns), end));
    Some(diagnostic(span, whole, comment))
}

/// A comment written as `#212` (no space after `#`), or after a value ending in a comma.
fn looks_cut(value: &str, comment: &str) -> bool {
    let after_hash = comment.chars().nth(1);
    after_hash.is_some_and(|c| !c.is_whitespace()) || value.ends_with(',')
}

fn diagnostic(span: SourceSpan, whole: &str, comment: &str) -> Diagnostic {
    Diagnostic::new(Code::V3204, span, format!("YAML reads `{comment}` as a comment, cutting the value short"))
        .label("the value ends before the `#`")
        .help("quote the value to keep the `#` and what follows it")
        .fix(Fix::suggested("Quote the value", vec![TextEdit::replace(span, double_quote(whole))]))
}
