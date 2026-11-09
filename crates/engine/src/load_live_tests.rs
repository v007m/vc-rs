use std::time::Duration;

use serde_json::json;
use vc_executor::{IterationCall, IterationReport};
use vc_model::{ApiId, Part, StringTable, Template, TemplateValue};

use super::{kind, literal};

#[test]
fn an_iteration_fails_by_its_first_unanswered_or_error_call_else_its_last() {
    let mut strings = StringTable::new();
    let [first, second, third] = ["a#one", "a#two", "a#three"].map(|name| ApiId::new(strings.intern(name).unwrap()));
    let call = |api, status| IterationCall { api: Some(api), status, elapsed: Duration::ZERO };
    let report = |calls| IterationReport { calls, ..IterationReport::default() };
    let errored = report(vec![call(first, Some(200)), call(second, Some(503)), call(third, None)]);
    assert_eq!(kind(&errored), (Some(second), Some(503)));
    assert_eq!(kind(&report(vec![call(first, Some(200)), call(third, None)])), (Some(third), None));
    assert_eq!(kind(&report(vec![call(first, Some(200)), call(second, Some(399))])), (Some(second), Some(399)));
    assert_eq!(kind(&report(vec![call(first, Some(400))])), (Some(first), Some(400)));
    assert_eq!(kind(&report(Vec::new())), (None, None));
}

#[test]
fn a_threshold_value_renders_only_when_it_is_literal() {
    let text = TemplateValue::String(Template { parts: vec![Part::Literal("a".into())] });
    let list = TemplateValue::List(vec![TemplateValue::Null, TemplateValue::Bool(true), text]);
    assert_eq!(literal(&list), Some(json!([null, true, "a"])));
    let map = TemplateValue::Map(std::iter::once(("lt".to_owned(), TemplateValue::Number(5.into()))).collect());
    assert_eq!(literal(&map), Some(json!({"lt": 5})));
    let empty = Template { parts: Vec::new() };
    let extend = TemplateValue::Extend { base: empty, changes: Box::new(TemplateValue::Null), remove: Vec::new() };
    assert_eq!(literal(&TemplateValue::List(vec![extend])), None);
}
