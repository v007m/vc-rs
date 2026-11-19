//! A `graphql` response's errors as `expect.errors` reads them (D345): each error normalised to
//! `{message, path, code, extensions}`, with `path` dotted as body paths are written and `code`
//! its `extensions.code`; and the internal `fields` matcher an expected error lowers to (D360).

use std::fmt::Write as _;

use serde_json::{Map, Value};
use vc_model::MatcherArg;

use super::{Judge, MatcherRegistry};

/// `registry` with `fields`, which holds when each field's matcher holds on that field.
pub fn register(registry: MatcherRegistry) -> MatcherRegistry {
    registry.with("fields", fields)
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

/// The errors of a response `body`, normalised; an empty list when it has none.
pub fn normalised(body: Option<&Value>) -> Value {
    let errors = body.and_then(|body| body.get("errors")).and_then(Value::as_array);
    Value::Array(errors.into_iter().flatten().map(error).collect())
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

/// `["users", 2, "email"]` as `users[2].email`.
fn dotted(path: &[Value]) -> String {
    let mut text = String::new();
    for segment in path {
        match segment {
            Value::Number(index) => {
                let _ = write!(text, "[{index}]");
            }
            Value::String(key) if text.is_empty() => text.push_str(key),
            Value::String(key) => {
                text.push('.');
                text.push_str(key);
            }
            other => {
                let _ = write!(text, ".{other}");
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use vc_model::{
        ActionId, Check, CheckKind, Matcher, MatcherArg, MatcherCall, Origin, Pos, SourceFileId, SourceSpan,
        TemplateValue,
    };
    use vc_template::RenderError;

    use super::super::{Arguments, Judge, MatcherRegistry, Subject, evaluate};
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

    fn call(name: &str, arg: MatcherArg) -> Matcher {
        let span = SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0));
        Matcher::All(vec![MatcherCall { name: name.to_owned(), arg, span }])
    }

    /// `{ some: <an entry of these fields> }`, as the compiler lowers it (D360).
    fn some(fields: &[(&str, &str)]) -> Check {
        let equals = |text: &str| Matcher::Equals(TemplateValue::String(fixture::literal(text)));
        let fields = fields.iter().map(|(name, text)| ((*name).to_owned(), equals(text))).collect();
        let matcher = call("some", MatcherArg::Matcher(Box::new(call("fields", MatcherArg::Fields(fields)))));
        Check {
            kind: CheckKind::Errors,
            target: None,
            matcher,
            origin: Origin::Action(ActionId::new(0)),
            span: matcher_span(),
        }
    }

    fn matcher_span() -> SourceSpan {
        SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0))
    }

    #[test]
    fn an_entry_holds_when_every_field_it_writes_does_and_a_failure_is_v5123() {
        let response = json!({"body": {"errors": [{"message": "late", "path": ["user", "notes", 0], "extensions": {"code": "TIMEOUT"}}]}});
        let checks = [
            some(&[("code", "TIMEOUT"), ("path", "user.notes[0]")]),
            some(&[("code", "TIMEOUT"), ("message", "early")]),
            some(&[("message", "early"), ("code", "TIMEOUT")]),
        ];
        let registry = MatcherRegistry::standard();
        let results = evaluate(&Judge::new(&registry, &Texts), &checks, Subject { response: &response, size: 0 });
        let verdicts: Vec<_> = results.iter().map(|result| (result.passed, result.code.clone())).collect();
        assert_eq!(verdicts, [(true, None), (false, Some("V5123".to_owned())), (false, Some("V5123".to_owned()))]);
        assert_eq!(results[1].expected, json!({"some": {"fields": {"code": "TIMEOUT", "message": "early"}}}));
        assert_eq!(
            results[1].actual,
            Some(
                json!([{"message": "late", "path": "user.notes[0]", "code": "TIMEOUT", "extensions": {"code": "TIMEOUT"}}])
            )
        );
    }

    #[test]
    fn fields_takes_only_a_matcher_per_field() {
        let registry = MatcherRegistry::standard();
        let judge = Judge::new(&registry, &Texts);
        let wrong = call("fields", MatcherArg::Matchers(vec![]));
        assert!(judge.matches(&wrong, Some(&json!({}))).is_err());
    }

    #[test]
    fn normalises_each_error_to_the_fields_it_has_with_a_dotted_path() {
        let body = json!({"data": null, "errors": [
            {"message": "no", "path": ["users", 2, "email"], "extensions": {"code": "FORBIDDEN", "ms": 5}},
            {"message": "odd", "path": [true, "x"]},
            {"path": "not a list"},
        ]});
        let expected = json!([
            {"message": "no", "path": "users[2].email", "code": "FORBIDDEN", "extensions": {"code": "FORBIDDEN", "ms": 5}},
            {"message": "odd", "path": ".true.x"},
            {},
        ]);
        assert_eq!(normalised(Some(&body)), expected);
    }

    #[test]
    fn a_body_without_errors_has_an_empty_list() {
        for body in [json!({"data": {}}), json!({"errors": "boom"}), json!([1])] {
            assert_eq!(normalised(Some(&body)), json!([]), "{body}");
        }
        assert_eq!(normalised(None), json!([]));
    }
}
