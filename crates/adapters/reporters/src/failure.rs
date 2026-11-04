//! The detailed block under a failed test: one compiler-style diagnostic per failed check, with
//! the source line underlined, or the failure's message when no check failed.

use std::fmt::Write as _;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Render as _, TextRenderer};
use vc_model::{CheckResult, Failure, Origin};

use crate::collect::TestRecord;
use crate::names::{self, Names};
use crate::{Color, encode, text};

/// The indent of a block under its test line.
const INDENT: &str = "  ";

/// Every failed check of `record`, else its failure; empty when it has neither.
pub fn block(names: &Names, record: &TestRecord, color: Color) -> String {
    let mut out = String::new();
    for check in record.failed_checks() {
        out.push_str(&self::check(names, check, color));
    }
    if out.is_empty()
        && let Some(failed) = &record.result.failure
    {
        out.push_str(&failure(names, failed, color));
    }
    indent(&out)
}

/// One failed check, as a diagnostic at its `expect` line when the program has the file.
pub fn check(names: &Names, check: &CheckResult, color: Color) -> String {
    let code = check.code.as_deref().and_then(|code| code.parse::<Code>().ok()).unwrap_or(Code::V5101);
    let message = format!("expect.{}", names::check_label(check));
    let mut diagnostic = Diagnostic::new(code, check.span, message).label(detail(check));
    if let Origin::Api(_) = check.origin {
        diagnostic = diagnostic.note(format!("expected by {}", encode::origin(names, check.origin)));
    }
    let shown = names.location(check.span).and_then(|_| render(names, diagnostic.clone(), color));
    let mut out = shown.unwrap_or_else(|| plain(&diagnostic, color));
    if let Some(diff) = &check.diff {
        out.push_str("   = diff:\n");
        for line in text::sanitize(diff).lines() {
            let _ = writeln!(out, "     {line}");
        }
    }
    out
}

/// `expected 400, got 201`, or the error the check hit.
fn detail(check: &CheckResult) -> String {
    if let Some(error) = &check.error {
        return error.clone();
    }
    let actual = check.actual.as_ref().map_or_else(|| "nothing".to_owned(), text::value);
    format!("expected {}, got {actual}", text::value(&check.expected))
}

/// A failure no check explains: a transport error, a timeout, a setup that failed.
pub fn failure(names: &Names, failure: &Failure, color: Color) -> String {
    let code = failure.code.as_deref().unwrap_or("error");
    let head = paint(&format!("error[{}]", text::sanitize(code)), color);
    let mut out = format!("{head}: {}\n", text::sanitize(&failure.message));
    if let Some(action) = failure.action {
        let _ = writeln!(out, "   = at: {}", text::sanitize(&names.action(action)));
    }
    out
}

fn render(names: &Names, diagnostic: Diagnostic, color: Color) -> Option<String> {
    let diagnostics = Diagnostics::from_iter([diagnostic]);
    let shown = TextRenderer::default().color(color.is_on()).render(&diagnostics, &names.program().files).ok()?;
    Some(format!("{}\n", shown.trim_end()))
}

/// The diagnostic without an excerpt, for a span in a file the program does not have.
fn plain(diagnostic: &Diagnostic, color: Color) -> String {
    let head = paint(&format!("{}[{}]", diagnostic.severity, diagnostic.code), color);
    let label = text::sanitize(&diagnostic.primary.text);
    let mut out = format!("{head}: {}: {label}\n", text::sanitize(&diagnostic.message));
    for note in &diagnostic.notes {
        let _ = writeln!(out, "   = note: {}", text::sanitize(note));
    }
    out
}

fn paint(text: &str, color: Color) -> String {
    if color.is_on() { format!("\x1b[1;31m{text}\x1b[0m") } else { text.to_owned() }
}

fn indent(block: &str) -> String {
    block.lines().map(|line| if line.is_empty() { "\n".to_owned() } else { format!("{INDENT}{line}\n") }).collect()
}
