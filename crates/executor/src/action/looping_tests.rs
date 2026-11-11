use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::time::Instant;
use vc_diagnostics::Code;
use vc_interfaces::{SendContext, Transport, TransportError};
use vc_model::{
    ActionId, ConnectionInfo, Exchange, HttpResponse, Loop, OwnerKey, RenderedRequest, TemplateValue, Timing,
};
use vc_testkit::FakeTransport;

use crate::fixture::{Fixture, Server};
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PendingCleanup, PipelineParts};

/// Answers each request with its path as `{"path": …}`, after `/<n>` milliseconds × 10 for a path
/// ending in a number.
struct Paths;

#[async_trait]
impl Transport for Paths {
    async fn send(&self, request: RenderedRequest, _: &SendContext) -> Result<Exchange, TransportError> {
        let path = request.uri.path().to_owned();
        let wait = path.rsplit('/').next().and_then(|last| last.parse::<u64>().ok()).unwrap_or(0);
        tokio::time::sleep(Duration::from_millis(wait * 10)).await;
        let body = Bytes::from(json!({ "path": path }).to_string());
        let response = HttpResponse { wire_size: body.len() as u64, body, ..vc_testkit::status(200) };
        Ok(Exchange { request, response, timing: Timing::default(), connection: ConnectionInfo::default() })
    }
}

/// Runs `action` with a pipeline over `transport`.
async fn run(fixture: Fixture, action: ActionId, transport: Arc<dyn Transport>) -> ActionReport {
    let executors = ActionExecutors::standard(PipelineParts::new(transport, HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

fn list(values: &[Value]) -> Value {
    Value::Array(values.to_vec())
}

#[tokio::test(start_paused = true)]
async fn repeat_runs_each_turn_in_order_with_its_iteration_and_saves_a_list_per_name() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.api_mut(call).path = fixture.template("/users/${iteration.count}/${iteration.index}");
    fixture.action_mut(call).save.values =
        fixture.vars(&[("path", "${response.body.path}"), ("n", "${iteration.index}")]);
    fixture.action_mut(call).repeat = Some(Loop::Repeat { times: 3, parallel: 1 });
    let start = Instant::now();
    let report = run(fixture, call, Arc::new(Paths)).await;
    assert_eq!(report.outcome.failure(), None);
    let paths = [json!("/users/3/1"), json!("/users/3/2"), json!("/users/3/3")];
    assert_eq!(report.saved.get("path"), Some(&list(&paths)));
    assert_eq!(report.saved.get("n"), Some(&json!([1, 2, 3])));
    assert_eq!(start.elapsed(), Duration::from_millis(60), "one after another");
    assert_eq!(report.response.map(|response| response["body"]["path"].clone()), Some(json!("/users/3/3")));
    assert_eq!(report.checks.len(), 3, "each turn's default status check");
}

#[tokio::test(start_paused = true)]
async fn each_turn_draws_its_own_fake_vars() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    fixture.api_mut(call).path = fixture.template("/users/${var.userId}");
    fixture.action_mut(call).vars.values = fixture.vars(&[("userId", "${fake.string.uuid}")]);
    fixture.action_mut(call).save.values = fixture.vars(&[("path", "${response.body.path}")]);
    fixture.action_mut(call).repeat = Some(Loop::Repeat { times: 2, parallel: 1 });
    let report = run(fixture, call, Arc::new(Paths)).await;
    assert_eq!(report.outcome.failure(), None);
    let paths = report.saved.get("path").and_then(Value::as_array).cloned().unwrap_or_default();
    assert_eq!(paths.len(), 2);
    assert_ne!(paths[0], paths[1], "{paths:?}");
}

#[tokio::test(start_paused = true)]
async fn for_each_binds_item_and_keeps_turn_order_when_turns_run_at_once() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/items");
    fixture.api_mut(call).path = fixture.template("/items/${item.wait}");
    fixture.action_mut(call).save.values = fixture.vars(&[("path", "${response.body.path}")]);
    let items =
        [3, 1, 2].map(|wait| TemplateValue::Map([("wait".to_owned(), TemplateValue::Number(wait.into()))].into()));
    fixture.action_mut(call).repeat = Some(Loop::ForEach { items: TemplateValue::List(items.to_vec()), parallel: 3 });
    let start = Instant::now();
    let report = run(fixture, call, Arc::new(Paths)).await;
    assert_eq!(report.outcome.failure(), None);
    assert_eq!(report.saved.get("path"), Some(&json!(["/items/3", "/items/1", "/items/2"])));
    assert_eq!(start.elapsed(), Duration::from_millis(30), "all three at once");
}

#[tokio::test(start_paused = true)]
async fn turns_run_at_most_parallel_at_a_time() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/items");
    fixture.api_mut(call).path = fixture.template("/items/${item}");
    let items = [1, 1, 1, 1, 1].map(|wait| TemplateValue::Number(wait.into()));
    fixture.action_mut(call).repeat = Some(Loop::ForEach { items: TemplateValue::List(items.to_vec()), parallel: 2 });
    let start = Instant::now();
    let report = run(fixture, call, Arc::new(Paths)).await;
    assert_eq!((report.outcome.failure(), start.elapsed()), (None, Duration::from_millis(30)));
}

#[tokio::test]
async fn a_failed_turn_starts_no_more_and_keeps_the_cleanups_of_the_turns_that_ran() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users");
    let create = fixture.creating("/users", delete);
    fixture.action_mut(create).save.values = fixture.vars(&[("n", "${iteration.index}")]);
    fixture.action_mut(create).repeat = Some(Loop::Repeat { times: 4, parallel: 1 });
    let transport = FakeTransport::scripted([201, 201, 500, 201].map(|code| Ok(vc_testkit::status(code))).to_vec());
    let report = run(fixture, create, Arc::new(transport.clone())).await;
    let failure = report.outcome.failure().cloned().unwrap_or_else(|| unreachable!("the third turn fails"));
    assert!(failure.message.starts_with("iteration 3: "), "{}", failure.message);
    assert_eq!((failure.code, failure.action), (Some(Code::V5101.to_string()), Some(create)));
    assert_eq!(report.cleanups, vec![PendingCleanup::new(delete); 2]);
    assert_eq!((transport.calls(), report.saved.len()), (3, 0));
}

#[tokio::test]
async fn a_failed_turn_marks_its_own_exchanges_not_the_last_turn_s() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/items");
    fixture.api_mut(call).path = fixture.template("/${item}");
    let items = ["missing", "a", "b"].map(|item| fixture.value(item));
    fixture.action_mut(call).repeat = Some(Loop::ForEach { items: TemplateValue::List(items.to_vec()), parallel: 3 });
    let (server, _sent) = Server::new();
    let report = run(fixture, call, server).await;
    let statuses: Vec<u16> = report.exchanges.iter().map(|exchange| exchange.response.status.as_u16()).collect();
    assert_eq!((statuses, report.failing), (vec![404, 200, 200], Some(0..1)));
    assert!(report.outcome.failure().is_some_and(|failure| failure.message.starts_with("iteration 1: ")));
}

#[tokio::test]
async fn for_each_needs_a_list() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/items");
    fixture.program.config.vars.values = fixture.vars(&[("cities", "Oslo")]);
    let items = fixture.value("${var.cities}");
    fixture.action_mut(call).repeat = Some(Loop::ForEach { items, parallel: 1 });
    let report = run(fixture, call, Arc::new(FakeTransport::scripted(vec![]))).await;
    let failure = report.outcome.failure().cloned();
    assert_eq!(
        failure.map(|failure| (failure.code, failure.action)),
        Some((Some(Code::V5111.to_string()), Some(call)))
    );
    let mut fixture = Fixture::new();
    let call = fixture.call("/items");
    let items = fixture.value("${var.gone}");
    fixture.action_mut(call).repeat = Some(Loop::ForEach { items, parallel: 0 });
    let report = run(fixture, call, Arc::new(FakeTransport::scripted(vec![]))).await;
    assert_eq!(report.outcome.failure().and_then(|failure| failure.code.clone()), Some(Code::V5111.to_string()));
}

#[tokio::test]
async fn a_loop_with_no_turns_saves_each_name_as_an_empty_list() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/items");
    fixture.api_mut(call).save.values = fixture.vars(&[("lastId", "${response.body.id}")]);
    fixture.action_mut(call).save.values = fixture.vars(&[("path", "${response.body.path}")]);
    fixture.action_mut(call).repeat = Some(Loop::ForEach { items: TemplateValue::List(Vec::new()), parallel: 1 });
    let transport = FakeTransport::scripted(vec![]);
    let report = run(fixture, call, Arc::new(transport.clone())).await;
    assert_eq!(report.outcome.failure(), None);
    assert_eq!(report.saved.get("path"), Some(&json!([])));
    assert_eq!(report.saved.get("lastId"), Some(&json!([])));
    assert_eq!((transport.calls(), report.checks.len()), (0, 0));
}
