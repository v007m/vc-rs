//! A `graphql` response's errors as `expect.errors` reads them (D345): each error normalised to
//! `{message, path, code, extensions}`, with `path` dotted as body paths are written and `code`
//! its `extensions.code`; and the internal matchers the written forms lower to: `fields` for an
//! expected error (D360) and `entries` for the list form, which pairs each entry with its own
//! error (D362). A report shows each as written.

use serde_json::{Map, Value};
use vc_model::{Matcher, MatcherArg, Segment};

use super::{Judge, MatcherRegistry};

/// `registry` with `fields`, which holds when each field's matcher holds on that field, and
/// `entries`, which holds when the errors pair one to one with the entries.
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
    let errors = listed(actual);
    Ok(errors.len() == entries.len() && unpaired(judge, entries, errors)?.is_empty())
}

fn listed(actual: Option<&Value>) -> &[Value] {
    actual.and_then(Value::as_array).map_or(&[][..], Vec::as_slice)
}

/// The entries left without an error of their own once each is paired with a distinct error
/// it matches, as many as can be (Kuhn's augmenting paths).
fn unpaired(judge: &Judge<'_>, entries: &[Matcher], errors: &[Value]) -> Result<Vec<usize>, String> {
    let fits = entries.iter().map(|entry| fits(judge, entry, errors)).collect::<Result<_, _>>()?;
    let mut pairing = Pairing { fits, owner: vec![None; errors.len()], seen: Vec::new() };
    Ok((0..entries.len()).filter(|&entry| !pairing.paired(entry)).collect())
}

/// The indexes of the errors `entry` matches.
fn fits(judge: &Judge<'_>, entry: &Matcher, errors: &[Value]) -> Result<Vec<usize>, String> {
    let mut fits = Vec::new();
    for (index, error) in errors.iter().enumerate() {
        if judge.matches(entry, Some(error))? {
            fits.push(index);
        }
    }
    Ok(fits)
}

/// Each entry's fitting errors, the entry each error is paired with, and the errors one search
/// has tried.
struct Pairing {
    fits: Vec<Vec<usize>>,
    owner: Vec<Option<usize>>,
    seen: Vec<bool>,
}

impl Pairing {
    /// Pairs `entry` with an error, moving earlier entries to other errors where that frees one.
    fn paired(&mut self, entry: usize) -> bool {
        self.seen = vec![false; self.owner.len()];
        self.augment(entry)
    }

    fn augment(&mut self, entry: usize) -> bool {
        for index in 0..self.fits[entry].len() {
            let error = self.fits[entry][index];
            if std::mem::replace(&mut self.seen[error], true) {
                continue;
            }
            if self.owner[error].is_none_or(|other| self.augment(other)) {
                self.owner[error] = Some(entry);
                return true;
            }
        }
        false
    }
}

/// What a failed list-form check reports as its actual value: the errors with their `count`
/// when it is not the number of entries, or else with the entries, as written, that no error
/// was left for (`unpaired`). `None` for any other matcher.
pub fn missed(judge: &Judge<'_>, matcher: &Matcher, errors: &Value) -> Option<Value> {
    let entries = list_form(matcher)?;
    let listed = listed(Some(errors));
    let shown = if listed.len() == entries.len() {
        let unpaired = unpaired(judge, entries, listed).ok()?;
        let written = unpaired.iter().map(|&entry| judge.expected(&entries[entry])).collect::<Result<_, _>>();
        ("unpaired", Value::Array(written.ok()?))
    } else {
        ("count", Value::from(listed.len()))
    };
    Some(Value::Object(Map::from_iter([("errors".to_owned(), errors.clone()), (shown.0.to_owned(), shown.1)])))
}

/// The entries of the list form, when `matcher` is one.
fn list_form(matcher: &Matcher) -> Option<&[Matcher]> {
    let Matcher::All(calls) = matcher else { return None };
    match calls.as_slice() {
        [call] if call.name == "entries" => match &call.arg {
            MatcherArg::Matchers(entries) => Some(entries),
            _ => None,
        },
        _ => None,
    }
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
        None | Some(Value::Null) => Ok(Value::Array(Vec::new())),
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
        let errors = json!([{"message": "m", "code": "A", "extensions": {"code": "A"}}, {"code": "A", "extensions": {"code": "A"}}]);
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
        let expected = json!([
            {"message": "no", "path": "users[2].email", "code": "FORBIDDEN", "extensions": {"code": "FORBIDDEN", "ms": 5}},
            {"message": "odd", "path": "true.x[\"a.b\"][\"1.5\"][-1]"},
            {},
        ]);
        assert_eq!(normalised(Some(&body)), Ok(expected));
    }

    #[test]
    fn a_response_without_errors_or_with_null_has_an_empty_list_and_anything_else_is_no_graphql_response() {
        assert_eq!(normalised(Some(&json!({"data": {}}))), Ok(json!([])));
        assert_eq!(normalised(Some(&json!({"errors": []}))), Ok(json!([])));
        assert_eq!(normalised(Some(&json!({"errors": null}))), Ok(json!([])), "null is no errors (D363)");
        assert_eq!(passed(&judged(&[listed(&[])], &json!({"data": null, "errors": null}))), [(true, None)]);
        assert_eq!(normalised(Some(&json!({"errors": {}}))), Err("`errors` is not a list"));
        assert_eq!(normalised(Some(&json!(1))), Err("the body is not a GraphQL response"));
        assert_eq!(normalised(None), Err("the body is not a GraphQL response"));
    }
}
