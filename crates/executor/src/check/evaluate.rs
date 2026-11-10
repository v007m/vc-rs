//! A call's checks evaluated against its response, each reported on its own.

use serde_json::Value;
use similar::TextDiff;
use vc_diagnostics::Code;
use vc_model::{Check, CheckKind, CheckResult, Failure, Matcher};

use super::{Judge, schema};

/// How many of the ways a body breaks its schema a failed `body.schema` check reports.
const SHOWN: usize = 5;

/// What a call's checks read: the response as `${response}` binds it, and the body's size in
/// bytes.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Subject<'s> {
    /// `status`, `headers` (lowercase names), `body` and `timing` (milliseconds).
    pub response: &'s Value,
    /// The body's size in bytes.
    pub size: u64,
}

/// Each of `checks` against `subject`, in order.
pub fn evaluate(judge: &Judge<'_>, checks: &[Check], subject: Subject<'_>) -> Vec<CheckResult> {
    checks.iter().map(|check| evaluated(judge, check, subject)).collect()
}

/// The failure of the first check that did not hold, with its code.
pub fn verdict(results: &[CheckResult]) -> Option<Failure> {
    let failed = results.iter().find(|result| !result.passed)?;
    let failure = Failure::new(message(failed));
    Some(match &failed.code {
        Some(code) => failure.with_code(code.clone()),
        None => failure,
    })
}

/// What a check read and how it went, before it is reported.
struct Outcome {
    actual: Option<Value>,
    expected: Result<Value, String>,
    verdict: Result<bool, String>,
    code: Code,
}

fn evaluated(judge: &Judge<'_>, check: &Check, subject: Subject<'_>) -> CheckResult {
    let Outcome { actual, expected, verdict, code } = outcome(judge, check, subject);
    let passed = verdict == Ok(true);
    let error = verdict.err().or_else(|| expected.as_ref().err().cloned());
    let expected = expected.unwrap_or(Value::Null);
    let compared = matches!(check.matcher, Matcher::Equals(_)) && !passed;
    CheckResult {
        kind: check.kind,
        target: check.target.clone(),
        origin: check.origin,
        span: check.span,
        passed,
        diff: actual.as_ref().filter(|_| compared).and_then(|actual| diff(&expected, actual)),
        expected,
        actual,
        error,
        code: (!passed).then(|| code.to_string()),
    }
}

/// The one place a check's kind decides what it reads and how it is judged.
fn outcome(judge: &Judge<'_>, check: &Check, subject: Subject<'_>) -> Outcome {
    let at = |root: &str| {
        let path = check.target.as_ref().map_or(&[][..], |path| path.segments.as_slice());
        subject.response.get(root).and_then(|value| vc_template::select(root, value, path).ok())
    };
    match check.kind {
        CheckKind::Status => matched(judge, check, at("status")),
        CheckKind::Header => matched(judge, check, at("headers")),
        CheckKind::Timing => matched(judge, check, at("timing")),
        CheckKind::Match | CheckKind::Equals => matched(judge, check, at("body")),
        CheckKind::Size => matched(judge, check, Some(Value::from(subject.size))),
        CheckKind::Cel => cel(judge, &check.matcher),
        CheckKind::Schema => schema_check(judge, check, subject.response.get("body")),
        CheckKind::Custom => pending("the custom check's function did not run", Code::V5103),
    }
}

fn matched(judge: &Judge<'_>, check: &Check, actual: Option<Value>) -> Outcome {
    let verdict = judge.matches(&check.matcher, actual.as_ref());
    Outcome { actual, expected: judge.expected(&check.matcher), verdict, code: Code::V5101 }
}

/// A CEL check: the expression must evaluate to `true`.
fn cel(judge: &Judge<'_>, matcher: &Matcher) -> Outcome {
    let Matcher::Equals(expression) = matcher else { return pending("a CEL check holds one expression", Code::V5102) };
    let actual = judge.render(expression);
    let verdict = actual.as_ref().map(|value| *value == Value::Bool(true)).map_err(Clone::clone);
    Outcome { actual: actual.ok(), expected: Ok(Value::Bool(true)), verdict, code: Code::V5102 }
}

/// A `body.schema` check: the body must match the JSON Schema the check names (D201). Up to
/// [`SHOWN`] of the ways it does not are reported.
fn schema_check(judge: &Judge<'_>, check: &Check, body: Option<&Value>) -> Outcome {
    let Some(key) = schema::key(check) else { return pending("a schema check names one schema", Code::V5101) };
    let verdict = judge.schema(key, body.unwrap_or(&Value::Null)).and_then(|broken| reported(&broken));
    Outcome { actual: body.cloned(), expected: Ok(Value::String(key.to_owned())), verdict, code: Code::V5101 }
}

/// Holds when nothing is `broken`; otherwise fails with the first [`SHOWN`] reasons.
fn reported(broken: &[String]) -> Result<bool, String> {
    if broken.is_empty() {
        return Ok(true);
    }
    let shown = broken.iter().take(SHOWN).map(String::as_str).collect::<Vec<_>>().join("; ");
    let more = broken.len().saturating_sub(SHOWN);
    Err(if more == 0 { shown } else { format!("{shown} (and {more} more)") })
}

fn pending(message: &str, code: Code) -> Outcome {
    Outcome { actual: None, expected: Ok(Value::Null), verdict: Err(message.to_owned()), code }
}

/// A unified diff of two objects or lists as pretty JSON; `None` for anything else.
pub fn diff(expected: &Value, actual: &Value) -> Option<String> {
    let structured = |value: &Value| value.is_object() || value.is_array();
    if !structured(expected) || !structured(actual) {
        return None;
    }
    let pretty = |value: &Value| serde_json::to_string_pretty(value).unwrap_or_default();
    let (expected, actual) = (pretty(expected), pretty(actual));
    Some(TextDiff::from_lines(&expected, &actual).unified_diff().header("expected", "actual").to_string())
}

/// `status: expected ["2xx"], got 404`, with the diff or the reason it could not be evaluated.
fn message(result: &CheckResult) -> String {
    let kind = format!("{:?}", result.kind).to_lowercase();
    let label = format!(
        "{kind}{}",
        result.target.as_ref().map_or_else(String::new, |path| vc_template::path_text(&path.segments))
    );
    if let Some(error) = &result.error {
        return format!("{label}: {error}");
    }
    let got = result.actual.as_ref().map_or_else(|| "nothing".to_owned(), vc_template::preview);
    let message = format!("{label}: expected {}, got {got}", vc_template::preview(&result.expected));
    match &result.diff {
        Some(diff) => format!("{message}\n{diff}"),
        None => message,
    }
}
