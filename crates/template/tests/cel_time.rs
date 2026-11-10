#![cfg(test)]

use serde_json::{Value, json};
use vc_model::{CelSource, Namespace, Pos, ScalarStyle, SourceFileId, SourceSpan, StringTable};
use vc_template::{Activation, CelCompiler, ExprError, Roots};

fn run(text: &str) -> Result<Value, ExprError> {
    let span = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::new(1, 1, text.len()));
    let source = CelSource { text: text.into(), span, style: ScalarStyle::Plain };
    let compiler = CelCompiler::new();
    let program = compiler.check(&source, &Roots::new().with_call(), &mut StringTable::new()).unwrap();
    let vars = json!({"at": "2024-05-01T12:30:00Z", "late": "2024-05-01T14:30:00+01:00", "wait": "90s"});
    compiler.prepare(&program).unwrap().eval(&Activation::new().with(Namespace::Var, vars))
}

fn value(text: &str) -> Value {
    run(text).unwrap_or_else(|error| panic!("{text}: {error:?}"))
}

fn failure(text: &str) -> String {
    match run(text) {
        Err(ExprError::Eval(message)) => message,
        other => panic!("{text}: {other:?}"),
    }
}

#[test]
fn timestamps_render_as_rfc_3339_in_utc() {
    let cases = [
        ("2024-05-01T12:30:00Z", "2024-05-01T12:30:00Z"),
        ("2024-05-01t14:30:00.5+02:00", "2024-05-01T12:30:00.5Z"),
        ("2024-05-01 07:00:00.000000001-05:30", "2024-05-01T12:30:00.000000001Z"),
        ("2024-02-29T23:59:59z", "2024-02-29T23:59:59Z"),
        ("2000-02-29T00:00:00Z", "2000-02-29T00:00:00Z"),
        ("1969-12-31T23:59:59.9Z", "1969-12-31T23:59:59.9Z"),
        ("0001-01-01T00:00:00Z", "0001-01-01T00:00:00Z"),
        ("9999-12-31T23:59:59.999999999Z", "9999-12-31T23:59:59.999999999Z"),
        ("2023-12-31T23:30:00-01:00", "2024-01-01T00:30:00Z"),
        ("2023-03-01T00:00:00+00:00", "2023-03-01T00:00:00Z"),
        ("1600-11-30T00:00:00Z", "1600-11-30T00:00:00Z"),
    ];
    for (written, shown) in cases {
        assert_eq!(value(&format!("timestamp('{written}')")), json!(shown), "{written}");
        assert_eq!(value(&format!("string(timestamp('{written}'))")), json!(shown), "{written}");
    }
}

#[test]
fn durations_render_in_seconds() {
    let cases = [
        ("1h30m", "5400s"),
        ("1.5s", "1.5s"),
        ("-300ms", "-0.3s"),
        ("+2m", "120s"),
        ("0", "0s"),
        ("-0", "0s"),
        ("1us", "0.000001s"),
        ("1\u{b5}s1\u{3bc}s", "0.000002s"),
        ("7ns", "0.000000007s"),
        (".5h", "1800s"),
        ("2.h", "7200s"),
        ("1.0000000000000000009s", "1s"),
        ("1h1m1s1ms", "3661.001s"),
    ];
    for (written, shown) in cases {
        assert_eq!(value(&format!("duration('{written}')")), json!(shown), "{written}");
        assert_eq!(value(&format!("string(duration('{written}'))")), json!(shown), "{written}");
    }
}

#[test]
fn bad_timestamps_say_what_is_wrong() {
    let shape = "is not an RFC 3339 timestamp such as `2024-05-01T12:30:00Z`";
    for text in ["2024-05-01", "2024-05-01T12:30Z", "2024-05-01T12:30:00", "2024-05-01T12:30:00.Z", "x"] {
        assert!(failure(&format!("timestamp('' + '{text}')")).contains(shape), "{text}");
    }
    let cases = [
        ("2024-05-01T12:30:00.1234567890Z", shape),
        ("2024-05-01T12:30:00+24:00", shape),
        ("2024-05-01T12:30:00+01:60", shape),
        ("2024-05-01T12:30:00Z ", shape),
        ("2024-13-01T00:00:00Z", "is not a calendar date"),
        ("2023-02-29T00:00:00Z", "is not a calendar date"),
        ("1900-02-29T00:00:00Z", "is not a calendar date"),
        ("2024-04-31T00:00:00Z", "is not a calendar date"),
        ("0000-01-01T00:00:00Z", "is not a calendar date"),
        ("2024-01-00T00:00:00Z", "is not a calendar date"),
        ("2024-01-01T24:00:00Z", "is not a time of day"),
        ("2024-01-01T23:60:00Z", "is not a time of day"),
        ("2024-01-01T23:59:60Z", "is not a time of day"),
        ("0001-01-01T00:00:00+00:01", "is outside years 1 to 9999"),
        ("9999-12-31T23:59:59-00:01", "is outside years 1 to 9999"),
    ];
    for (text, reason) in cases {
        let message = failure(&format!("timestamp('' + '{text}')"));
        assert!(message.contains(reason), "{text}: {message}");
    }
    assert!(failure("timestamp(1)").contains("timestamp"));
}

#[test]
fn bad_durations_say_what_is_wrong() {
    let shape = "is not a duration such as `1h30m` or `1.5s`";
    for text in ["", "-", "1", "h", "1d", "1.2.3s", ".s", "1h-1m", "99999999999999999999999999999999999999999h"] {
        let message = failure(&format!("duration('' + '{text}')"));
        assert!(message.contains(shape), "{text}: {message}");
    }
    assert!(failure("duration('' + '2562048h')").contains("is too long"));
    assert_eq!(value("duration('' + '2562047h') > duration('1h')"), json!(true));
}

#[test]
fn times_compare_add_and_subtract() {
    let cases = [
        ("timestamp(var.at) < timestamp(var.late)", json!(true)),
        ("timestamp(var.at) <= timestamp(var.at)", json!(true)),
        ("timestamp(var.late) > timestamp(var.at)", json!(true)),
        ("timestamp(var.at) >= timestamp(var.late)", json!(false)),
        ("timestamp(var.at) == timestamp('2024-05-01T13:30:00+01:00')", json!(true)),
        ("timestamp(var.at) != timestamp(var.late)", json!(true)),
        ("timestamp(var.at) == duration('1s')", json!(false)),
        ("duration('1m') == duration('60s')", json!(true)),
        ("duration('1m') < duration(var.wait)", json!(true)),
        ("timestamp(var.late) - timestamp(var.at)", json!("3600s")),
        ("timestamp(var.at) - timestamp(var.late)", json!("-3600s")),
        ("timestamp(var.at) + duration(var.wait)", json!("2024-05-01T12:31:30Z")),
        ("duration(var.wait) + timestamp(var.at)", json!("2024-05-01T12:31:30Z")),
        ("timestamp(var.at) - duration('1h')", json!("2024-05-01T11:30:00Z")),
        ("duration('1h') + duration(var.wait)", json!("3690s")),
        ("duration('1h') - duration(var.wait)", json!("3510s")),
        ("[timestamp(var.at)].size()", json!(1)),
    ];
    for (text, expected) in cases {
        assert_eq!(value(text), expected, "{text}");
    }
}

#[test]
fn mixed_or_overflowing_time_arithmetic_fails() {
    let cases = [
        ("timestamp(var.at) + timestamp(var.at)", "no `+` for google.protobuf.Timestamp and google.protobuf.Timestamp"),
        ("timestamp(var.at) - 1", "no `-` for google.protobuf.Timestamp and int"),
        ("timestamp(var.at) < duration('1s')", "google.protobuf.Timestamp and google.protobuf.Duration"),
        ("duration('1s') < 1", "no `<` for google.protobuf.Duration and int"),
        ("duration('1s') + 1", "no `+` for google.protobuf.Duration and int"),
        ("duration('1s') - timestamp(var.at)", "no `-` for google.protobuf.Duration"),
        ("timestamp('9999-12-31T00:00:00Z') + duration('24h')", "out of range"),
        ("timestamp('0001-01-01T00:00:00Z') - duration('1ns')", "out of range"),
        ("duration(var.wait) + timestamp('9999-12-31T23:59:00Z')", "out of range"),
        ("timestamp('9999-01-01T00:00:00Z') - timestamp('0001-01-01T00:00:00Z')", "out of range"),
        ("duration('2562047h') + duration('2562047h')", "out of range"),
        ("duration('-2562047h') - duration('2562047h')", "out of range"),
        ("string(duration('1s') == duration('1s'))", "true"),
    ];
    for (text, reason) in cases {
        let shown = run(text).map_or_else(|error| format!("{error:?}"), |value| value.to_string());
        assert!(shown.contains(reason), "{text}: {shown}");
    }
}

#[test]
fn a_time_inside_a_list_is_not_json() {
    assert!(matches!(run("[timestamp(var.at)]"), Err(ExprError::Eval(_))));
    assert_eq!(value("[var.at].map(t, string(timestamp(t)))"), json!(["2024-05-01T12:30:00Z"]));
}
