use std::sync::Arc;
use std::time::Duration;

use http::{Method, Uri};
use serde_json::json;
use tokio::sync::mpsc;
use vc_executor::{IterationCall, IterationFailure, IterationReport};
use vc_model::{
    ActionId, ApiId, ConnectionInfo, Exchange, Part, Program, RenderedRequest, SourceFiles, StringTable, Template,
    TemplateValue, TestId, Timing,
};

use super::{Live, SAMPLES, kind, literal};

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

#[test]
fn each_error_kind_keeps_the_exchanges_of_its_first_five_failures_only() {
    let mut strings = StringTable::new();
    let [lists, gets] = ["a.f#lists", "a.f#gets"].map(|name| TestId::new(strings.intern(name).unwrap()));
    let program = Program::new(strings, SourceFiles::default());
    let exchange = |at: usize| Exchange {
        request: RenderedRequest::new(Method::GET, Uri::try_from(format!("http://x/{at}")).unwrap()),
        response: vc_testkit::status(500),
        timing: Timing::default(),
        connection: ConnectionInfo::default(),
    };
    let failed = |at: usize| IterationReport {
        failure: Some(IterationFailure {
            action: Some(ActionId::new(3)),
            exchange: Some(Box::new(exchange(at))),
            ..IterationFailure::default()
        }),
        ..IterationReport::default()
    };
    let (send, mut received) = mpsc::unbounded_channel();
    (0..7).for_each(|at| send.send((lists, failed(at))).unwrap());
    send.send((gets, failed(7))).unwrap();
    send.send((gets, IterationReport { failure: Some(IterationFailure::default()), ..IterationReport::default() }))
        .unwrap();
    let mut live = Live::new(Arc::new(program.clone()), None, None);
    live.count(&mut received);
    let kinds = live.error_kinds(&program);
    let counted: Vec<_> =
        kinds.iter().map(|(kind, samples)| (kind.test.as_deref(), kind.count, samples.len())).collect();
    assert_eq!(counted, [(Some("a.f#lists"), 7, SAMPLES), (Some("a.f#gets"), 2, 1)]);
    let urls: Vec<String> = kinds[0].1.iter().map(|(_, exchange)| exchange.request.uri.to_string()).collect();
    assert_eq!(urls, ["http://x/0", "http://x/1", "http://x/2", "http://x/3", "http://x/4"], "the first five");
    assert_eq!(kinds[1].1[0].0, Some(ActionId::new(3)));
    assert!(live.error_kinds(&program).is_empty(), "taken once");
}
