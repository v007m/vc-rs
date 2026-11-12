use std::time::Duration;

use serde_json::json;
use vc_executor::{IterationCall, IterationFailure, IterationReport};
use vc_model::{ApiId, Part, StringTable, Template, TemplateValue, TestId};

use super::{kind, literal};

#[test]
fn an_iteration_fails_by_its_test_and_the_action_that_failed_it() {
    let mut strings = StringTable::new();
    let [before, failed] = ["a#before", "a#failed"].map(|name| ApiId::new(strings.intern(name).unwrap()));
    let test = TestId::new(strings.intern("a.f#t").unwrap());
    let call = |api, status| IterationCall { api: Some(api), status, elapsed: Duration::ZERO };
    let calls = vec![call(before, Some(500)), call(failed, Some(200)), call(before, Some(204))];
    let blamed =
        |failure| IterationReport { calls: calls.clone(), failure: Some(failure), ..IterationReport::default() };
    let check =
        IterationFailure { api: Some(failed), status: Some(200), code: Some("V5101".into()), ..Default::default() };
    assert_eq!(kind(test, &blamed(check)), (Some(failed), Some(test), Some(200), Some("V5101".to_owned())));
    let command = IterationFailure { code: Some("V5110".into()), ..Default::default() };
    assert_eq!(kind(test, &blamed(command)), (None, Some(test), None, Some("V5110".to_owned())));
    assert_eq!(kind(test, &IterationReport::default()), (None, Some(test), None, None));
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
