use std::borrow::Cow;
use std::time::Duration;

use serde_json::Value;
use vc_model::TestStatus;

/// Shown in place of a character that is unsafe to print.
const SUBSTITUTE: char = '\u{fffd}';

/// `text` with control characters (tabs and newlines aside) and bidi overrides replaced, so a
/// response body or a message cannot move the cursor or reorder a terminal line.
pub fn sanitize(text: &str) -> Cow<'_, str> {
    if text.chars().any(is_unsafe) {
        Cow::Owned(text.chars().map(|c| if is_unsafe(c) { SUBSTITUTE } else { c }).collect())
    } else {
        Cow::Borrowed(text)
    }
}

const fn is_unsafe(c: char) -> bool {
    (c.is_control() && !matches!(c, '\t' | '\n'))
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{61c}')
}

/// A duration as people read it: `840ms`, `6.4s`, `2m 5s`.
pub fn duration(time: Duration) -> String {
    let millis = time.as_millis();
    match millis {
        0..1_000 => format!("{millis}ms"),
        1_000..60_000 => format!("{}.{}s", millis / 1_000, millis % 1_000 / 100),
        _ => format!("{}m {}s", millis / 60_000, millis % 60_000 / 1_000),
    }
}

/// `1 test`, `3 tests`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

/// A JSON value on one line, sanitized: `200`, `"Ada"`, `{"id":1}`.
pub fn value(value: &Value) -> String {
    sanitize(&value.to_string()).into_owned()
}

/// A status as reports name it: `passed`, `blocked`.
pub const fn status(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Passed => "passed",
        TestStatus::Failed => "failed",
        TestStatus::Blocked => "blocked",
        TestStatus::Flaky => "flaky",
        TestStatus::Skipped => "skipped",
        TestStatus::AllowedFailure => "allowedFailure",
    }
}

/// Milliseconds, saturating.
pub fn millis(time: Duration) -> u64 {
    u64::try_from(time.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use vc_model::TestStatus;

    use super::{count, duration, millis, sanitize, status, value};

    #[test]
    fn sanitize_replaces_controls_and_bidi_marks_only() {
        assert_eq!(sanitize("ok\tfine\nnext"), "ok\tfine\nnext");
        assert!(matches!(sanitize("plain"), std::borrow::Cow::Borrowed(_)));
        let text = "a\x1b[2Jb\rc\x7fd\u{202e}e\u{2066}f\u{200e}g\u{200f}h\u{61c}i\u{9b}j";
        assert_eq!(
            sanitize(text),
            "a\u{fffd}[2Jb\u{fffd}c\u{fffd}d\u{fffd}e\u{fffd}f\u{fffd}g\u{fffd}h\u{fffd}i\u{fffd}j"
        );
        assert_eq!(sanitize("\u{2029}\u{2065}\u{206a}\u{2028}"), "\u{2029}\u{2065}\u{206a}\u{2028}");
        assert_eq!(sanitize("\u{2029}\u{2069}\u{202a}"), "\u{2029}\u{fffd}\u{fffd}");
    }

    #[test]
    fn durations_read_naturally() {
        let shown = [0, 999, 1_000, 6_480, 59_999, 60_000, 125_400].map(|ms| duration(Duration::from_millis(ms)));
        assert_eq!(shown, ["0ms", "999ms", "1.0s", "6.4s", "59.9s", "1m 0s", "2m 5s"]);
        assert_eq!(millis(Duration::from_micros(2_900)), 2);
        assert_eq!(millis(Duration::MAX), u64::MAX);
    }

    #[test]
    fn counts_values_and_statuses_print_as_reports_show_them() {
        assert_eq!([count(0, "file"), count(1, "file"), count(2, "file")], ["0 files", "1 file", "2 files"]);
        assert_eq!(value(&json!({"name": "A\u{1b}"})), "{\"name\":\"A\\u001b\"}");
        assert_eq!(value(&json!("x\u{202e}")), "\"x\u{fffd}\"");
        let all = [
            TestStatus::Passed,
            TestStatus::Failed,
            TestStatus::Blocked,
            TestStatus::Flaky,
            TestStatus::Skipped,
            TestStatus::AllowedFailure,
        ];
        assert_eq!(all.map(status), ["passed", "failed", "blocked", "flaky", "skipped", "allowedFailure"]);
    }
}
