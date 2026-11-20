use serde_json::{Value, json};
use vc_model::{
    ActionId, Check, CheckKind, Matcher, MatcherArg, MatcherCall, Origin, Path, Pos, Segment, SourceFileId, SourceSpan,
    TemplateValue,
};
use vc_template::RenderError;

use super::super::{Arguments, Judge, MatcherRegistry, Subject, evaluate};
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

fn equals(text: &str) -> Matcher {
    Matcher::Equals(TemplateValue::String(fixture::literal(text)))
}

/// `{type: <kind>, data: {match: {<path>: <text>}}}`, as the compiler lowers it (D379).
fn message(kind: &str, (path, text): (&[&str], &str)) -> Matcher {
    let path = Path { segments: path.iter().map(|key| Segment::Key((*key).to_owned())).collect() };
    let data = call("paths", MatcherArg::Paths(vec![(path, equals(text))]));
    call("fields", MatcherArg::Fields(vec![("type".to_owned(), equals(kind)), ("data".to_owned(), data)]))
}

fn messages(name: &str, entry: Matcher) -> Check {
    let matcher = call(name, MatcherArg::Matcher(Box::new(entry)));
    Check { kind: CheckKind::Messages, target: None, matcher, origin: Origin::Action(ActionId::new(0)), span: span() }
}

fn judged(check: Check, messages: &Value) -> vc_model::CheckResult {
    let registry = MatcherRegistry::standard();
    let response = json!({"messages": messages});
    let mut results = evaluate(&Judge::new(&registry, &Texts), &[check], Subject { response: &response, size: 0 });
    results.remove(0)
}

fn received() -> Value {
    json!([
        {"type": "next", "data": {"user": {"id": "6"}}},
        {"type": "ack", "data": {"user": {"id": "7"}}},
        {"type": "ping", "data": "hello"},
    ])
}

#[test]
fn some_message_matches_when_one_has_each_field_and_each_data_path() {
    assert!(judged(messages("some", message("ack", (&["user", "id"], "7"))), &received()).passed);
}

#[test]
fn a_path_matches_only_on_the_message_whose_other_fields_match() {
    assert!(!judged(messages("some", message("ack", (&["user", "id"], "6"))), &received()).passed);
    assert!(!judged(messages("some", message("ack", (&["user", "name"], "7"))), &received()).passed);
}

#[test]
fn a_path_on_data_that_is_not_an_object_does_not_match() {
    let check = messages("every", message("ping", (&["user"], "hello")));
    let result = judged(check, &json!([{"type": "ping", "data": "hello"}]));
    assert!(!result.passed);
    assert_eq!(result.code.as_deref(), Some("V5101"));
}

#[test]
fn a_report_shows_each_path_dotted_with_what_it_expects() {
    let result = judged(messages("some", message("ack", (&["user", "id"], "8"))), &received());
    let expected = json!({"some": {"type": "ack", "data": {"user.id": "8"}}});
    assert_eq!(result.expected, expected);
}

#[test]
fn paths_needs_a_matcher_per_path() {
    let registry = MatcherRegistry::standard();
    let judge = Judge::new(&registry, &Texts);
    let wrong = call("paths", MatcherArg::Value(TemplateValue::Null));
    assert_eq!(judge.matches(&wrong, Some(&json!({}))), Err("expected a matcher per path".to_owned()));
}
