//! A `graphql` response's errors as `expect.errors` reads them (D345): each error normalised to
//! `{message, path, code, extensions}`, with `path` dotted as body paths are written and `code`
//! its `extensions.code`; and the internal matchers the written forms lower to: `fields` for an
//! expected error (D360) and `entries` for the list form. A report shows each as written.

use serde_json::{Map, Value};
use vc_model::{MatcherArg, Segment};

use super::{Judge, MatcherRegistry};

/// `registry` with `fields`, which holds when each field's matcher holds on that field, and
/// `entries`, which holds when each error matches some entry and each entry some error.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry.internal("fields", fields).internal("entries", entries)
}

fn fields(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let MatcherArg::Fields(fields) = arg else { return Err("expected a matcher per field".to_owned()) };
    for (name, matcher) in fields {
        if !judge.matches(matcher, actual.and_then(|object| object.get(name)))? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn entries(judge: &Judge<'_>, actual: Option<&Value>, arg: &MatcherArg) -> Result<bool, String> {
    let MatcherArg::Matchers(entries) = arg else { return Err("expected a list of expected errors".to_owned()) };
    let errors = actual.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    for error in errors {
        if !any(entries, |entry| judge.matches(entry, Some(error)))? {
            return Ok(false);
        }
    }
    for entry in entries {
        if !any(errors, |error| judge.matches(entry, Some(error)))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `holds` is true of one of `items`; the first error stops the search.
fn any<T>(items: &[T], holds: impl Fn(&T) -> Result<bool, String>) -> Result<bool, String> {
    for item in items {
        if holds(item)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The errors of a response `body`, normalised; an empty list when it has none.
///
/// # Errors
///
/// What the check reports as the actual value when `body` is not a GraphQL response (not a
/// JSON object) or its `errors` is not a list.
pub fn normalised(body: Option<&Value>) -> Result<Value, &'static str> {
    let Some(body) = body.and_then(Value::as_object) else { return Err("the body is not a GraphQL response") };
    match body.get("errors") {
        None => Ok(Value::Array(Vec::new())),
        Some(Value::Array(errors)) => Ok(Value::Array(errors.iter().map(error).collect())),
        Some(_) => Err("`errors` is not a list"),
    }
}

/// One error with the fields it has: `message`, `path` dotted, `code` and `extensions`.
fn error(error: &Value) -> Value {
    let extensions = error.get("extensions");
    let path = error.get("path").and_then(Value::as_array).map(|path| Value::String(dotted(path)));
    let fields = [
        ("message", error.get("message").cloned()),
        ("path", path),
        ("code", extensions.and_then(|extensions| extensions.get("code")).cloned()),
        ("extensions", extensions.cloned()),
    ];
    let present = fields.into_iter().filter_map(|(name, value)| Some((name.to_owned(), value?)));
    Value::Object(present.collect::<Map<_, _>>())
}

/// `["users", 2, "email"]` as `users[2].email`; a segment that is neither a key nor a whole
/// number is a key of its JSON text.
fn dotted(path: &[Value]) -> String {
    let segment = |value: &Value| match value {
        Value::String(key) => Segment::Key(key.clone()),
        Value::Number(number) => number.as_i64().map_or_else(|| Segment::Key(number.to_string()), Segment::Index),
        other => Segment::Key(other.to_string()),
    };
    vc_template::dotted(&path.iter().map(segment).collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vc_model::{
        ActionId, Check, CheckKind, Matcher, MatcherArg, MatcherCall, Origin, Pos, SourceFileId, SourceSpan,
        TemplateValue,
    };
    use vc_template::RenderError;

    use super::super::{Arguments, Judge, MatcherRegistry, Subject, evaluate, verdict};
    use super::normalised;
    use crate::fixture;

    /// Renders the literal texts these checks hold.
    struct Texts;

    impl Arguments for Texts {
        fn render(&self, value: &TemplateValue) -> Result<Value, RenderError> {
            let TemplateValue::String(text) = value else { return Err(RenderError::missing("not text")) };
            let [vc_model::Part::Literal(text)] = text.parts.as_slice() else { return Err(RenderError::missing("x")) };
            Ok(json!(text))
        }
    }

    fn span() -> SourceSpan {
        SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
    }

    fn call(name: &str, arg: MatcherArg) -> Matcher {
        Matcher::All(vec![MatcherCall { name: name.to_owned(), arg, span: span() }])
    }

    /// An expected error of these fields, as the compiler lowers it (D360).
    fn entry(fields: &[(&str, &str)]) -> Matcher {
        let equals = |text: &str| Matcher::Equals(TemplateValue::String(fixture::literal(text)));
        call(
            "fields",
            MatcherArg::Fields(fields.iter().map(|(name, text)| ((*name).to_owned(), equals(text))).collect()),
        )
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

    #[test]
    fn the_list_form_needs_each_error_listed_and_each_entry_matched_and_fails_as_written() {
        let body = json!({"errors": [{"extensions": {"code": "A"}}, {"extensions": {"code": "B"}}]});
        let checks = [
            listed(&[&[("code", "B")], &[("code", "A")]]),
            listed(&[&[("code", "A")]]),
            listed(&[&[("code", "A")], &[("code", "B")], &[("code", "C")]]),
            listed(&[]),
        ];
        let results = judged(&checks, &body);
        assert_eq!(passed(&results), [(true, None), failed(), failed(), failed()]);
        assert_eq!(passed(&judged(&checks[3..], &json!({"data": {}}))), [(true, None)]);
        let failure = verdict(&results[1..2]).map(|failure| failure.message);
        let got = r#"[{"code":"A","extensions":{"code":"A"}},{"code":"B","extensions":{"code":"B"}}]"#;
        assert_eq!(failure, Some(format!(r#"errors: expected [{{"code":"A"}}], got {got}"#)));
    }

    #[test]
    fn a_body_that_is_not_a_graphql_response_fails_with_what_it_is() {
        let checks = [listed(&[])];
        for (body, actual) in [
            (json!({"errors": "boom"}), "`errors` is not a list"),
            (json!({"errors": null}), "`errors` is not a list"),
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
        let expected = json!([
            {"message": "no", "path": "users[2].email", "code": "FORBIDDEN", "extensions": {"code": "FORBIDDEN", "ms": 5}},
            {"message": "odd", "path": "true.x[\"a.b\"][\"1.5\"][-1]"},
            {},
        ]);
        assert_eq!(normalised(Some(&body)), Ok(expected));
    }

    #[test]
    fn a_response_without_errors_has_an_empty_list_and_anything_else_is_no_graphql_response() {
        assert_eq!(normalised(Some(&json!({"data": {}}))), Ok(json!([])));
        assert_eq!(normalised(Some(&json!({"errors": []}))), Ok(json!([])));
        assert_eq!(normalised(Some(&json!({"errors": {}}))), Err("`errors` is not a list"));
        assert_eq!(normalised(Some(&json!(1))), Err("the body is not a GraphQL response"));
        assert_eq!(normalised(None), Err("the body is not a GraphQL response"));
    }
}
