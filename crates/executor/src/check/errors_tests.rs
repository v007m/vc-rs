use serde_json::{Value, json};
use vc_model::{ActionId, Check, CheckKind, Matcher, MatcherArg, Origin, TemplateValue};

use super::super::{Judge, MatcherRegistry, Subject, evaluate, verdict};
use super::normalised;
use crate::check::testing::{Texts, call, span};
use crate::fixture;

/// An expected error of these fields, as the compiler lowers it (D360).
fn entry(fields: &[(&str, &str)]) -> Matcher {
    let equals = |text: &str| Matcher::Equals(TemplateValue::String(fixture::literal(text)));
    call("fields", MatcherArg::Fields(fields.iter().map(|(name, text)| ((*name).to_owned(), equals(text))).collect()))
}

fn errors_check(matcher: Matcher) -> Check {
    Check { kind: CheckKind::Errors, target: None, matcher, origin: Origin::Action(ActionId::new(0)), span: span() }
}

/// `{ some: <an entry of these fields> }`.
fn some(fields: &[(&str, &str)]) -> Check {
    errors_check(call("some", MatcherArg::Matcher(Box::new(entry(fields)))))
}

/// The list form, `[<an entry>, …]`.
fn listed(entries: &[&[(&str, &str)]]) -> Check {
    errors_check(call("entries", MatcherArg::Matchers(entries.iter().map(|fields| entry(fields)).collect())))
}

fn judged(checks: &[Check], body: &Value) -> Vec<vc_model::CheckResult> {
    let registry = MatcherRegistry::standard();
    let response = json!({"body": body});
    evaluate(&Judge::new(&registry, &Texts), checks, Subject { response: &response, size: 0 })
}

fn passed(results: &[vc_model::CheckResult]) -> Vec<(bool, Option<String>)> {
    results.iter().map(|result| (result.passed, result.code.clone())).collect()
}

fn failed() -> (bool, Option<String>) {
    (false, Some("V5123".to_owned()))
}

#[test]
fn an_entry_holds_when_every_field_it_writes_does_and_a_failure_shows_it_as_written() {
    let body =
        json!({"errors": [{"message": "late", "path": ["user", "notes", 0], "extensions": {"code": "TIMEOUT"}}]});
    let checks = [
        some(&[("code", "TIMEOUT"), ("path", "user.notes[0]")]),
        some(&[("code", "TIMEOUT"), ("message", "early")]),
        some(&[("message", "early"), ("code", "TIMEOUT")]),
    ];
    let results = judged(&checks, &body);
    assert_eq!(passed(&results), [(true, None), failed(), failed()]);
    assert_eq!(results[1].expected, json!({"some": {"code": "TIMEOUT", "message": "early"}}));
    assert_eq!(
        results[1].actual,
        Some(
            json!([{"message": "late", "path": "user.notes[0]", "code": "TIMEOUT", "extensions": {"code": "TIMEOUT"}}])
        )
    );
}

fn code(code: &str) -> Value {
    json!({"extensions": {"code": code}})
}

#[test]
fn the_list_form_pairs_each_entry_with_its_own_error_in_any_order() {
    let body = json!({"errors": [{"message": "m", "extensions": {"code": "A"}}, code("A")]});
    let checks = [
        listed(&[&[("code", "A")], &[("code", "A"), ("message", "m")]]),
        listed(&[&[("code", "A"), ("message", "m")], &[("code", "A")]]),
        listed(&[&[("code", "A")], &[("code", "A")]]),
        listed(&[&[("code", "A"), ("message", "m")], &[("message", "m")]]),
    ];
    let results = judged(&checks, &body);
    assert_eq!(passed(&results), [(true, None), (true, None), (true, None), failed()]);
    let errors =
        json!([{"message": "m", "code": "A", "extensions": {"code": "A"}}, {"code": "A", "extensions": {"code": "A"}}]);
    assert_eq!(results[3].actual, Some(json!({"errors": errors, "unpaired": [{"message": "m"}]})));
    assert_eq!(passed(&judged(&[listed(&[])], &json!({"data": {}}))), [(true, None)]);
}

#[test]
fn an_entry_written_twice_needs_two_errors_and_one_error_never_fills_two_entries() {
    let twice = [listed(&[&[("code", "FORBIDDEN")], &[("code", "FORBIDDEN")]])];
    let results = judged(&twice, &json!({"errors": [code("FORBIDDEN")]}));
    assert_eq!(passed(&results), [failed()]);
    let errors = json!([{"code": "FORBIDDEN", "extensions": {"code": "FORBIDDEN"}}]);
    assert_eq!(results[0].actual, Some(json!({"count": 1, "errors": errors})));
    assert_eq!(passed(&judged(&twice, &json!({"errors": [code("FORBIDDEN"), code("FORBIDDEN")]}))), [(true, None)]);
    let both = [listed(&[&[("code", "FORBIDDEN")], &[("message", "no")]])];
    let one = json!({"errors": [{"message": "no", "extensions": {"code": "FORBIDDEN"}}]});
    assert_eq!(passed(&judged(&both, &one)), [failed()]);
}

#[test]
fn an_error_no_entry_matches_fails_naming_the_entry_left_without_one_as_written() {
    let body = json!({"errors": [code("A"), code("C")]});
    let results = judged(&[listed(&[&[("code", "A")], &[("code", "B")]])], &body);
    assert_eq!(passed(&results), [failed()]);
    assert_eq!(results[0].expected, json!([{"code": "A"}, {"code": "B"}]));
    let errors = json!([{"code": "A", "extensions": {"code": "A"}}, {"code": "C", "extensions": {"code": "C"}}]);
    assert_eq!(results[0].actual, Some(json!({"errors": errors, "unpaired": [{"code": "B"}]})));
    let failure = verdict(&results).map(|failure| failure.message);
    let got = r#"{"errors":[{"code":"A","extensions":{"code":"A"}},{"code":"C","extensions":{"code":"C"}}],"unpaired":[{"code":"B"}]}"#;
    assert_eq!(failure, Some(format!(r#"errors: expected [{{"code":"A"}},{{"code":"B"}}], got {got}"#)));
    let more = judged(&[listed(&[&[("code", "A")]])], &body);
    assert_eq!(more[0].actual, Some(json!({"count": 2, "errors": errors})));
    let some_b = call("some", MatcherArg::Matcher(Box::new(entry(&[("code", "B")]))));
    let all = judged(&[errors_check(call("allOf", MatcherArg::Matchers(vec![some_b.clone(), some_b])))], &body);
    assert_eq!(passed(&all), [failed()]);
    assert_eq!(all[0].actual, Some(errors), "another matcher shows the errors");
}

#[test]
fn a_body_that_is_not_a_graphql_response_fails_with_what_it_is() {
    let checks = [listed(&[])];
    for (body, actual) in [
        (json!({"errors": "boom"}), "`errors` is not a list"),
        (json!({"errors": 1}), "`errors` is not a list"),
        (json!({"errors": true}), "`errors` is not a list"),
        (json!({"errors": {"message": "no"}}), "`errors` is not a list"),
        (json!("<html>"), "the body is not a GraphQL response"),
        (json!([1]), "the body is not a GraphQL response"),
    ] {
        let results = judged(&checks, &body);
        assert_eq!(passed(&results), [failed()], "{body}");
        assert_eq!(results[0].actual, Some(json!(actual)), "{body}");
        assert_eq!(results[0].expected, json!([]), "{body}");
    }
}

#[test]
fn internal_matchers_take_only_their_own_argument() {
    let registry = MatcherRegistry::standard();
    let judge = Judge::new(&registry, &Texts);
    assert!(judge.matches(&call("fields", MatcherArg::Matchers(vec![])), Some(&json!({}))).is_err());
    assert!(judge.matches(&call("entries", MatcherArg::Fields(vec![])), Some(&json!([]))).is_err());
}

#[test]
fn normalises_each_error_to_the_fields_it_has_with_a_dotted_path() {
    let body = json!({"data": null, "errors": [
        {"message": "no", "path": ["users", 2, "email"], "extensions": {"code": "FORBIDDEN", "ms": 5}},
        {"message": "odd", "path": [true, "x", "a.b", 1.5, -1]},
        {"path": "not a list"},
    ]});
    let expected = vec![
        json!({"message": "no", "path": "users[2].email", "code": "FORBIDDEN", "extensions": {"code": "FORBIDDEN", "ms": 5}}),
        json!({"message": "odd", "path": "true.x[\"a.b\"][\"1.5\"][-1]"}),
        json!({}),
    ];
    assert_eq!(normalised(Some(&body)), Ok(expected));
}

#[test]
fn a_response_without_errors_or_with_null_has_an_empty_list_and_anything_else_is_no_graphql_response() {
    assert_eq!(normalised(Some(&json!({"data": {}}))), Ok(Vec::new()));
    assert_eq!(normalised(Some(&json!({"errors": []}))), Ok(Vec::new()));
    assert_eq!(normalised(Some(&json!({"errors": null}))), Ok(Vec::new()), "null is no errors (D363)");
    assert_eq!(passed(&judged(&[listed(&[])], &json!({"data": null, "errors": null}))), [(true, None)]);
    assert_eq!(normalised(Some(&json!({"errors": {}}))), Err("`errors` is not a list"));
    assert_eq!(normalised(Some(&json!(1))), Err("the body is not a GraphQL response"));
    assert_eq!(normalised(None), Err("the body is not a GraphQL response"));
}

#[test]
fn hundreds_of_entries_pair_with_as_many_errors_in_reverse_and_report_the_one_left() {
    let count = 500;
    let codes: Vec<_> = (0..count).map(|at| format!("C{at}")).collect();
    let written: Vec<_> = codes.iter().map(|text| [("code", text.as_str())]).collect();
    let check = listed(&written.iter().map(|fields| &fields[..]).collect::<Vec<_>>());
    let mut errors: Vec<_> = codes.iter().rev().map(|text| code(text)).collect();
    errors[0] = code("X");
    let results = judged(&[check], &json!({"errors": errors}));
    assert_eq!(passed(&results), [failed()]);
    assert_eq!(results[0].actual.as_ref().and_then(|actual| actual.get("unpaired")), Some(&json!([{"code": "C499"}])));
}
