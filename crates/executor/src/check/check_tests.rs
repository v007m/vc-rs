use serde_json::{Value, json};
use vc_model::{
    ActionId, Check, CheckKind, Duration, Matcher, MatcherArg, MatcherCall, Origin, Part, Path, Pos, Segment, Size,
    SourceFileId, SourceSpan, StatusPattern, TemplateValue,
};
use vc_template::RenderError;

use super::{Arguments, Judge, MatcherRegistry, Subject};
use crate::fixture;

/// Renders literals only; any reference fails.
struct Literal;

impl Arguments for Literal {
    fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
        plain(value)
    }
}

fn plain(value: &TemplateValue) -> Result<Value, RenderError> {
    Ok(match value {
        TemplateValue::Null => Value::Null,
        TemplateValue::Bool(flag) => Value::Bool(*flag),
        TemplateValue::Number(number) => Value::Number(number.clone()),
        TemplateValue::String(template) => match template.parts.as_slice() {
            [Part::Literal(text)] => Value::String(text.clone()),
            _ => return Err(RenderError::missing("`${var.gone}` is missing")),
        },
        TemplateValue::List(items) => Value::Array(items.iter().map(plain).collect::<Result<_, _>>()?),
        TemplateValue::Map(fields) => {
            Value::Object(fields.iter().map(|(key, value)| Ok((key.clone(), plain(value)?))).collect::<Result<_, _>>()?)
        }
        _ => return Err(RenderError::missing("not a literal")),
    })
}

fn template(value: Value) -> TemplateValue {
    match value {
        Value::Null => TemplateValue::Null,
        Value::Bool(flag) => TemplateValue::Bool(flag),
        Value::Number(number) => TemplateValue::Number(number),
        Value::String(text) => TemplateValue::String(fixture::literal(&text)),
        Value::Array(items) => TemplateValue::List(items.into_iter().map(template).collect()),
        Value::Object(fields) => {
            TemplateValue::Map(fields.into_iter().map(|(key, value)| (key, template(value))).collect())
        }
    }
}

fn span() -> SourceSpan {
    SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
}

fn arg(value: Value) -> MatcherArg {
    MatcherArg::Value(template(value))
}

fn all(calls: Vec<(&str, MatcherArg)>) -> Matcher {
    Matcher::All(
        calls.into_iter().map(|(name, arg)| MatcherCall { name: name.to_owned(), arg, span: span() }).collect(),
    )
}

fn one(name: &str, value: Value) -> Matcher {
    all(vec![(name, arg(value))])
}

fn nested(name: &str, matcher: Matcher) -> Matcher {
    all(vec![(name, MatcherArg::Matcher(Box::new(matcher)))])
}

fn judge<T>(probe: impl FnOnce(&Judge<'_>) -> T) -> T {
    let registry = MatcherRegistry::standard();
    probe(&Judge::new(&registry, &Literal))
}

fn passes(matcher: &Matcher, actual: Option<Value>) -> Result<bool, String> {
    judge(move |judge| judge.matches(matcher, actual.as_ref()))
}

fn holds(matcher: &Matcher, actual: Value) -> bool {
    passes(matcher, Some(actual)) == Ok(true)
}

/// A matcher, the actual value (`None`: absent) and the outcome expected.
type Case = (Matcher, Option<Value>, Option<bool>);

/// Each case's matcher against its actual value (`None`: absent): `Some(held)`, or `None` when
/// the matcher cannot be used.
fn outcomes(cases: Vec<Case>) {
    for (matcher, actual, expected) in cases {
        let shown = format!("{matcher:?} on {actual:?}");
        assert_eq!(passes(&matcher, actual).ok(), expected, "{shown}");
    }
}

#[test]
fn equality_is_deep_and_numbers_compare_by_value() {
    outcomes(vec![
        (one("eq", json!(1)), Some(json!(1.0)), Some(true)),
        (one("eq", json!({"a": [1, 2]})), Some(json!({"a": [1.0, 2]})), Some(true)),
        (one("eq", json!({"a": [1, 2]})), Some(json!({"a": [1, 2, 3]})), Some(false)),
        (one("eq", json!({"a": 1})), Some(json!({"a": 1, "b": 2})), Some(false)),
        (one("eq", json!({"a": 1})), Some(json!({"b": 1})), Some(false)),
        (one("eq", json!("a")), Some(json!("b")), Some(false)),
        (one("ne", json!(null)), Some(json!(0)), Some(true)),
        (one("ne", json!(2)), Some(json!(2)), Some(false)),
        (one("eq", json!(null)), None, Some(false)),
        (one("ne", json!(null)), None, Some(true)),
        (Matcher::Equals(template(json!([1, "x"]))), Some(json!([1, "x"])), Some(true)),
    ]);
}

#[test]
fn orderings_compare_numbers_or_strings_at_their_bounds() {
    let cases = [
        ("gt", [false, false, true]),
        ("gte", [false, true, true]),
        ("lt", [true, false, false]),
        ("lte", [true, true, false]),
    ];
    for (name, expected) in cases {
        let found = [4, 5, 6].map(|actual| holds(&one(name, json!(5)), json!(actual)));
        assert_eq!(found, expected, "{name}");
    }
    assert!(holds(&one("lt", json!("2024-02-01")), json!("2024-01-31")));
    assert!(!holds(&one("gt", json!(1)), json!("2")));
    assert_eq!(passes(&one("gt", json!(1)), None), Ok(false));
}

#[test]
fn between_is_inclusive_and_in_takes_a_list() {
    let range = |low: Value, high: Value| all(vec![("between", MatcherArg::Range(Box::new([arg(low), arg(high)])))]);
    let found = [0, 1, 5, 10, 11].map(|actual| holds(&range(json!(1), json!(10)), json!(actual)));
    assert_eq!(found, [false, true, true, true, false]);
    assert!(passes(&one("between", json!(1)), Some(json!(1))).is_err());
    assert!(holds(&one("in", json!(["active", "pending"])), json!("pending")));
    assert!(!holds(&one("in", json!(["active"])), json!("gone")));
    assert!(holds(&one("notIn", json!(["active"])), json!("gone")));
    assert!(!holds(&one("notIn", json!(["active"])), json!("active")));
    assert!(passes(&one("in", json!("active")), Some(json!("active"))).is_err());
}

#[test]
fn type_exists_and_empty_read_the_value_shape() {
    let types = [
        ("string", json!("a")),
        ("number", json!(1.5)),
        ("integer", json!(2.0)),
        ("integer", json!(-3)),
        ("boolean", json!(false)),
        ("object", json!({})),
        ("array", json!([])),
        ("null", json!(null)),
    ];
    for (name, value) in types {
        assert!(holds(&one("type", json!(name)), value), "{name}");
    }
    outcomes(vec![
        (one("type", json!("integer")), Some(json!(2.5)), Some(false)),
        (one("type", json!("string")), Some(json!(1)), Some(false)),
        (one("type", json!("date")), Some(json!(1)), None),
        (one("exists", json!(true)), Some(json!(null)), Some(true)),
        (one("exists", json!(false)), None, Some(true)),
        (one("exists", json!(true)), None, Some(false)),
        (one("exists", json!("yes")), None, None),
        (one("empty", json!(true)), Some(json!("")), Some(true)),
        (one("empty", json!(false)), Some(json!({"a": 1})), Some(true)),
        (one("empty", json!(true)), Some(json!([1])), Some(false)),
        (one("empty", json!(true)), Some(json!(null)), Some(false)),
    ]);
}

#[test]
fn length_takes_a_number_or_a_matcher() {
    assert!(holds(&one("length", json!(3)), json!("héé")));
    assert!(holds(&one("length", json!(2)), json!([1, 2])));
    assert!(!holds(&one("length", json!(2)), json!({"a": 1})));
    assert!(holds(&nested("length", one("gte", json!(1))), json!([0])));
    assert!(!holds(&nested("length", one("gte", json!(1))), json!([])));
    assert!(!holds(&one("length", json!(0)), json!(7)));
}

#[test]
fn formats_match_their_patterns() {
    let cases = [
        ("email", "a.b@example.com", "a@b"),
        ("uuid", "123e4567-e89b-12d3-a456-426614174000", "123e4567"),
        ("uri", "https://example.com/x", "not a uri"),
        ("date", "2024-02-29", "2024-13-01"),
        ("dateTime", "2024-02-29T10:00:00.5Z", "2024-02-29 25:00:00Z"),
        ("ipv4", "192.168.0.255", "256.1.1.1"),
    ];
    for (name, good, bad) in cases {
        assert!(holds(&one("format", json!(name)), json!(good)), "{name}");
        assert!(!holds(&one("format", json!(name)), json!(bad)), "{name}");
    }
    assert!(!holds(&one("format", json!("email")), json!(1)));
    assert!(passes(&one("format", json!("phone")), Some(json!("1"))).is_err());
}

#[test]
fn text_matchers_read_strings_and_contains_reads_lists() {
    outcomes(vec![
        (one("contains", json!("json")), Some(json!("application/json")), Some(true)),
        (one("contains", json!(2)), Some(json!([1, 2.0])), Some(true)),
        (one("contains", json!(3)), Some(json!([1, 2])), Some(false)),
        (one("contains", json!("a")), Some(json!({"a": 1})), Some(false)),
        (one("startsWith", json!("app")), Some(json!("application")), Some(true)),
        (one("startsWith", json!("tion")), Some(json!("application")), Some(false)),
        (one("endsWith", json!(".com")), Some(json!("x.com")), Some(true)),
        (one("endsWith", json!("x")), Some(json!("x.com")), Some(false)),
        (one("endsWith", json!("1")), Some(json!(1)), Some(false)),
        (one("regex", json!("^[A-Z]{3}-\\d+$")), Some(json!("ABC-12")), Some(true)),
        (one("regex", json!("^[A-Z]{3}$")), Some(json!("AB")), Some(false)),
        (one("regex", json!("(")), Some(json!("x")), None),
        (one("regex", json!(1)), Some(json!("x")), None),
    ]);
}

#[test]
fn list_matchers_apply_a_matcher_to_each_element() {
    let positive = || one("gt", json!(0));
    let found = [json!([1, 2]), json!([1, -1]), json!([-1]), json!([])]
        .map(|list| ["every", "some", "none"].map(|name| holds(&nested(name, positive()), list.clone())));
    assert_eq!(found, [[true, true, false], [false, true, false], [false, false, true], [true, false, true]]);
    assert!(!holds(&nested("every", positive()), json!(1)));
    assert!(passes(&one("every", json!(1)), Some(json!([1]))).is_err());
}

#[test]
fn not_all_of_and_any_of_combine_matchers() {
    assert!(holds(&nested("not", one("contains", json!("test"))), json!("prod")));
    assert!(!holds(&nested("not", one("contains", json!("test"))), json!("a test")));
    let several = |name: &str| all(vec![(name, MatcherArg::Matchers(vec![one("eq", json!(0)), one("gt", json!(10))]))]);
    let found =
        [0, 5, 11].map(|actual| [holds(&several("allOf"), json!(actual)), holds(&several("anyOf"), json!(actual))]);
    assert_eq!(found, [[false, true], [false, false], [false, true]]);
    let both = all(vec![("allOf", MatcherArg::Matchers(vec![one("gt", json!(1)), one("lt", json!(5))]))]);
    assert!(holds(&both, json!(3)));
    assert!(passes(&one("anyOf", json!(1)), Some(json!(1))).is_err());
    assert!(passes(&one("not", json!(1)), Some(json!(1))).is_err());
}

#[test]
fn several_matchers_in_one_object_must_all_pass_and_unknown_ones_fail() {
    let both = all(vec![("type", arg(json!("string"))), ("length", arg(json!(2)))]);
    assert!(holds(&both, json!("ab")));
    assert!(!holds(&both, json!("abc")));
    assert!(!holds(&both, json!(12)));
    assert_eq!(passes(&one("near", json!(1)), Some(json!(1))), Err("no matcher is named `near`".to_owned()));
    let status = Matcher::Status(vec![StatusPattern::Class(2), StatusPattern::Code(404)]);
    assert_eq!([200, 404, 500].map(|code| holds(&status, json!(code))), [true, true, false]);
    assert!(!holds(&status, json!("200")));
}

#[test]
fn expected_values_show_units_ranges_and_status_classes() {
    let matcher = all(vec![
        ("lt", MatcherArg::Duration(Duration::new(std::time::Duration::from_millis(500)))),
        ("lte", MatcherArg::Size(Size::from_bytes(2048))),
        ("between", MatcherArg::Range(Box::new([arg(json!(1)), arg(json!(2))]))),
        ("anyOf", MatcherArg::Matchers(vec![one("eq", json!(0))])),
    ]);
    let expected = judge(|judge| judge.expected(&matcher));
    assert_eq!(expected, Ok(json!({"lt": 500.0, "lte": 2048, "between": [1, 2], "anyOf": [{"eq": 0}]})));
    let status = Matcher::Status(vec![StatusPattern::Class(2), StatusPattern::Code(201)]);
    assert_eq!(judge(|judge| judge.expected(&status)), Ok(json!(["2xx", 201])));
    let gone = Matcher::Equals(TemplateValue::String(vc_model::Template { parts: vec![] }));
    assert!(judge(|judge| judge.expected(&gone)).is_err());
}

fn check(kind: CheckKind, target: &[&str], matcher: Matcher) -> Check {
    let segments = target.iter().map(|key| Segment::Key((*key).to_owned())).collect();
    let target = (!target.is_empty()).then_some(Path { segments });
    Check { kind, target, matcher, origin: Origin::Action(ActionId::new(0)), span: span() }
}

fn response() -> Value {
    json!({
        "status": 201,
        "headers": {"content-type": "application/json"},
        "body": {"id": "u1", "tags": ["a", "b"]},
        "timing": {"total": 120.0},
    })
}

fn results(checks: &[Check]) -> Vec<vc_model::CheckResult> {
    let response = response();
    judge(|judge| super::evaluate(judge, checks, Subject { response: &response, size: 40 }))
}

#[test]
fn each_check_reads_its_part_of_the_response() {
    let checks = [
        check(CheckKind::Status, &[], Matcher::Status(vec![StatusPattern::Code(201)])),
        check(CheckKind::Header, &["content-type"], one("contains", json!("json"))),
        check(CheckKind::Match, &["tags"], one("length", json!(2))),
        check(CheckKind::Equals, &[], Matcher::Equals(template(json!({"id": "u1", "tags": ["a", "b"]})))),
        check(CheckKind::Size, &[], one("lt", json!(41))),
        check(CheckKind::Timing, &["total"], one("lt", json!(500))),
        check(CheckKind::Cel, &["created"], Matcher::Equals(TemplateValue::Bool(true))),
    ];
    let results = results(&checks);
    assert!(results.iter().all(|result| result.passed && result.code.is_none()), "{results:#?}");
    assert_eq!(results[1].actual, Some(json!("application/json")));
    assert_eq!(results[4].actual, Some(json!(40)));
    assert_eq!(super::verdict(&results), None);
}

#[test]
fn a_failed_check_reports_expected_actual_a_diff_and_its_code() {
    let checks = [
        check(CheckKind::Match, &["missing"], one("exists", json!(true))),
        check(CheckKind::Equals, &[], Matcher::Equals(template(json!({"id": "u2"})))),
        check(CheckKind::Status, &[], Matcher::Status(vec![StatusPattern::Class(2)])),
    ];
    let results = results(&checks);
    assert_eq!(results.iter().map(|result| result.passed).collect::<Vec<_>>(), [false, false, true]);
    assert_eq!((results[0].actual.clone(), results[0].diff.clone()), (None, None));
    assert_eq!(results[1].expected, json!({"id": "u2"}));
    let diff = results[1].diff.clone().unwrap_or_default();
    assert!(diff.contains("-  \"id\": \"u2\"") && diff.contains("+  \"id\": \"u1\""), "{diff}");
    let failure = super::verdict(&results).unwrap();
    assert_eq!(failure.code.as_deref(), Some("V5101"));
    assert_eq!(failure.message, "match.missing: expected {\"exists\":true}, got nothing");
    let second = super::verdict(&results[1..]).unwrap();
    assert!(second.message.starts_with("equals: expected {\"id\":\"u2\"}, got {") && second.message.contains(&diff));
}

#[test]
fn cel_and_custom_checks_fail_with_their_own_codes() {
    let checks = [
        check(CheckKind::Cel, &["has items"], Matcher::Equals(TemplateValue::Bool(false))),
        check(CheckKind::Custom, &["hooks/checks.ts#isSorted"], Matcher::Equals(TemplateValue::Bool(true))),
        check(CheckKind::Schema, &[], Matcher::Equals(TemplateValue::Null)),
        check(CheckKind::Cel, &["x"], one("eq", json!(1))),
        check(CheckKind::Match, &["id"], Matcher::Equals(TemplateValue::String(vc_model::Template { parts: vec![] }))),
    ];
    let results = results(&checks);
    let codes: Vec<_> = results.iter().map(|result| result.code.clone().unwrap_or_default()).collect();
    assert_eq!(codes, ["V5102", "V5103", "V5101", "V5102", "V5101"]);
    assert_eq!((results[0].expected.clone(), results[0].actual.clone()), (json!(true), Some(json!(false))));
    let failure = super::verdict(&results[1..]).unwrap();
    assert_eq!(failure.message, "custom[\"hooks/checks.ts#isSorted\"]: the custom check's function did not run");
    assert!(results[4].error.as_deref().is_some_and(|error| error.contains("missing")));
}

#[test]
fn only_a_failed_equals_between_two_objects_or_lists_carries_a_diff() {
    let checks = [
        check(CheckKind::Equals, &[], Matcher::Equals(template(json!({"id": "u1", "tags": ["a", "b"]})))),
        check(CheckKind::Match, &[], one("length", json!(3))),
        check(CheckKind::Equals, &["id"], Matcher::Equals(template(json!({"id": "u1"})))),
        check(CheckKind::Equals, &["tags"], Matcher::Equals(template(json!(["a"])))),
    ];
    let results = results(&checks);
    let diffs: Vec<_> = results.iter().map(|result| result.diff.is_some()).collect();
    assert_eq!(diffs, [false, false, false, true]);
}

#[test]
fn a_schema_check_without_the_run_s_schemas_fails_as_not_loaded() {
    let key = Matcher::Equals(TemplateValue::String(fixture::literal("schemas/user.json")));
    let results = results(&[check(CheckKind::Schema, &[], key)]);
    let error = results[0].error.as_deref();
    assert_eq!(
        (error, results[0].code.as_deref()),
        (Some("no schema is loaded for `schemas/user.json`"), Some("V5101"))
    );
}
