//! A check's matcher as text: its names alone for `vc plan --explain`, or with the values they
//! expect for `vc explain`, which shows those values over the vars in force (D88).

use vc_model::{Check, CheckKind, Duration, Matcher, MatcherArg, MatcherCall, Size, StatusPattern, TemplateValue};

/// How a value is shown; with none, only the matchers' names are.
pub type Show<'a> = Option<&'a dyn Fn(&TemplateValue) -> String>;

/// A check's matcher: `lt 500ms`, a CEL check's expression, or a value it equals as written
/// (`{id: 1}`), which reads `equals` when values are not shown.
pub fn text(check: &Check, show: Show<'_>) -> String {
    match (&check.kind, &check.matcher) {
        (CheckKind::Cel, Matcher::Equals(TemplateValue::Cel(program))) => program.source.text.clone(),
        (_, matcher) => self::matcher(matcher, show),
    }
}

fn matcher(matcher: &Matcher, show: Show<'_>) -> String {
    match matcher {
        Matcher::Equals(value) => show.map_or_else(|| "equals".to_owned(), |show| show(value)),
        Matcher::StatusOf(value) => show.map_or_else(|| "status".to_owned(), |show| show(value)),
        Matcher::All(calls) => calls.iter().map(|call| self::call(call, show)).collect::<Vec<_>>().join(", "),
        Matcher::Status(patterns) => patterns.iter().map(|pattern| status(*pattern)).collect::<Vec<_>>().join(", "),
    }
}

fn call(call: &MatcherCall, show: Show<'_>) -> String {
    named(&call.name, show.map(|show| arg(&call.arg, show)))
}

/// A matcher's argument as written: `500ms`, `4kb`, `[1, 10]`, `{gt: 0}`.
fn arg(arg: &MatcherArg, show: &dyn Fn(&TemplateValue) -> String) -> String {
    match arg {
        MatcherArg::Value(value) => show(value),
        MatcherArg::Duration(duration) => self::duration(*duration),
        MatcherArg::Size(size) => self::size(*size),
        MatcherArg::Range(bounds) => {
            let [low, high] = bounds.as_ref();
            format!("[{}, {}]", self::arg(low, show), self::arg(high, show))
        }
        MatcherArg::Matcher(inner) => format!("{{{}}}", matcher(inner, Some(show))),
        MatcherArg::Matchers(inner) => {
            let inner: Vec<String> = inner.iter().map(|inner| format!("{{{}}}", matcher(inner, Some(show)))).collect();
            format!("[{}]", inner.join(", "))
        }
        MatcherArg::Fields(fields) => {
            let fields: Vec<String> =
                fields.iter().map(|(name, inner)| format!("{name}: {{{}}}", matcher(inner, Some(show)))).collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}

fn named(name: &str, value: Option<String>) -> String {
    value.map_or_else(|| name.to_owned(), |value| format!("{name} {value}"))
}

/// `2s` when whole seconds, else `250ms`.
fn duration(duration: Duration) -> String {
    let millis = duration.get().as_millis();
    if millis > 0 && millis.is_multiple_of(1000) { format!("{}s", millis / 1000) } else { format!("{millis}ms") }
}

/// The largest binary unit the size is a whole number of: `4kb`, else `32b` (D146).
fn size(size: Size) -> String {
    let bytes = size.bytes();
    let units = [(1 << 30, "gb"), (1 << 20, "mb"), (1 << 10, "kb")];
    let unit = units.into_iter().find(|(unit, _)| bytes > 0 && bytes.is_multiple_of(*unit));
    unit.map_or_else(|| format!("{bytes}b"), |(unit, name)| format!("{}{name}", bytes / unit))
}

fn status(pattern: StatusPattern) -> String {
    match pattern {
        StatusPattern::Code(code) => code.to_string(),
        StatusPattern::Class(class) => format!("{class}xx"),
    }
}

#[cfg(test)]
mod tests {
    use super::{arg, duration, size};
    use vc_model::{Duration, Matcher, MatcherArg, Size, TemplateValue};

    #[test]
    fn units_are_written_out_as_a_file_writes_them() {
        let shown = |millis| duration(Duration::new(std::time::Duration::from_millis(millis)));
        assert_eq!(
            (shown(500), shown(2000), shown(1500), shown(0)),
            ("500ms".into(), "2s".into(), "1500ms".into(), "0ms".into())
        );
        let sized = |bytes| size(Size::from_bytes(bytes));
        assert_eq!(
            (sized(4096), sized(1 << 20), sized(32), sized(0)),
            ("4kb".into(), "1mb".into(), "32b".into(), "0b".into())
        );
    }

    #[test]
    fn an_error_entry_shows_each_field_with_what_it_expects() {
        let show = |value: &TemplateValue| match value {
            TemplateValue::Bool(flag) => flag.to_string(),
            _ => "?".to_owned(),
        };
        let fields = vec![
            ("code".to_owned(), Matcher::Equals(TemplateValue::Bool(true))),
            ("path".to_owned(), Matcher::Equals(TemplateValue::Bool(false))),
        ];
        assert_eq!(arg(&MatcherArg::Fields(fields), &show), "{code: {true}, path: {false}}");
    }
}
