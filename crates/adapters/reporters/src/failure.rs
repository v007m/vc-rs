//! The detailed block under a failed test: one compiler-style diagnostic per failed check, with
//! the source line underlined, or the failure's message when no check failed. A test that ran
//! more than once shows each different failure once, under the attempts that hit it.

use std::fmt::Write as _;

use vc_diagnostics::{Code, Diagnostic, Diagnostics, Render as _, TextRenderer};
use vc_model::{CheckResult, Failure, Names, Origin};

use crate::collect::{StepRecord, TestRecord};
use crate::{Color, encode, text};

/// The indent of a block under its test line.
const INDENT: &str = "  ";

/// Every failed check of `record`, else its failure; empty when it has neither. A retried test
/// shows each different failure once, headed `attempts 1, 2 and 3 of 3:`.
pub fn block(names: &Names, record: &TestRecord, color: Color) -> String {
    let total = record.result.attempts;
    let tries: Vec<&[StepRecord]> = record.steps.split_inclusive(|step| step.outcome.failure().is_some()).collect();
    if total <= 1 || tries.len() != usize::try_from(total).unwrap_or(0) {
        return indent(&single(names, record, color));
    }
    let mut shown: Vec<(Vec<usize>, String)> = Vec::new();
    for (at, steps) in tries.into_iter().enumerate() {
        let text = attempt(names, steps, color);
        match shown.iter_mut().find(|(_, seen)| *seen == text) {
            Some((attempts, _)) => attempts.push(at + 1),
            None => shown.push((vec![at + 1], text)),
        }
    }
    let shown = shown.into_iter().filter(|(_, text)| !text.is_empty());
    let out: String = shown.map(|(attempts, text)| format!("{}\n{text}", heading(&attempts, total))).collect();
    if out.is_empty() { indent(&single(names, record, color)) } else { indent(&out) }
}

/// The failed checks of the whole record, else its failure.
fn single(names: &Names, record: &TestRecord, color: Color) -> String {
    let mut out: String = record.failed_checks().map(|failed| check(names, failed, color)).collect();
    if out.is_empty()
        && let Some(failed) = &record.result.failure
    {
        out.push_str(&failure(names, failed, color));
    }
    out
}

/// The failed checks of one attempt's steps, else the failure of the step that ended it.
fn attempt(names: &Names, steps: &[StepRecord], color: Color) -> String {
    let checks = steps.iter().flat_map(|step| &step.checks).filter(|found| !found.passed);
    let out: String = checks.map(|failed| check(names, failed, color)).collect();
    if !out.is_empty() {
        return out;
    }
    steps.iter().find_map(|step| step.outcome.failure()).map(|failed| failure(names, failed, color)).unwrap_or_default()
}

/// `attempt 2 of 3:`, or `attempts 1, 2 and 3 of 3:`.
fn heading(attempts: &[usize], total: u32) -> String {
    match attempts {
        [one] => format!("attempt {one} of {total}:"),
        [rest @ .., last] => {
            let rest: Vec<String> = rest.iter().map(ToString::to_string).collect();
            format!("attempts {} and {last} of {total}:", rest.join(", "))
        }
        [] => String::new(),
    }
}

/// One failed check, as a diagnostic at its `expect` line when the program has the file.
pub fn check(names: &Names, check: &CheckResult, color: Color) -> String {
    let code = check.code.as_deref().and_then(|code| code.parse::<Code>().ok()).unwrap_or(Code::V5101);
    let message = format!("expect.{}", vc_model::check_label(check));
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
