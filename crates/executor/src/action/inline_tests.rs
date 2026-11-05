use std::sync::Arc;

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use vc_model::{ActionId, ActionKind, BodySpec, CheckKind, HttpResponse, InlineRequest, Method, Origin, OwnerKey};
use vc_testkit::{FakeTransport, Recorded, RecordingTransport};

use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PipelineParts};

fn json_response(code: u16, body: &Value) -> HttpResponse {
    let body = Bytes::from(body.to_string());
    let headers = HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_static("application/json"))]);
    HttpResponse { status: StatusCode::from_u16(code).unwrap(), headers, wire_size: body.len() as u64, body }
}

async fn send(fixture: Fixture, action: ActionId, answers: Vec<HttpResponse>) -> (ActionReport, Vec<Recorded>) {
    let fake = FakeTransport::scripted(answers.into_iter().map(Ok).collect());
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    (report, recordings.take())
}

/// An inline `POST` to `url` with an `x-step` header and the text body `hi ${var.name}`.
fn inline(fixture: &mut Fixture, url: &str) -> ActionId {
    let request = InlineRequest {
        method: Method::Post,
        url: fixture.template(url),
        headers: fixture.templates(&[("x-step", "${var.name}")]),
        body: BodySpec::Text(fixture.template("hi ${var.name}")),
    };
    let action = fixture.action(ActionKind::Inline(request));
    fixture.action_mut(action).vars.values = fixture.vars(&[("name", "ada")]);
    action
}

#[tokio::test]
async fn an_inline_request_is_sent_as_written_and_saves_when_its_checks_hold() {
    let mut fixture = Fixture::new();
    fixture.program.config.defaults.headers = fixture.templates(&[("x-step", "config"), ("x-trace", "t")]);
    let action = inline(&mut fixture, "http://id.test/login?as=${var.name}");
    fixture.action_mut(action).save.values = fixture.vars(&[("token", "${response.body.token}")]);
    let (report, sent) = send(fixture, action, vec![json_response(201, &json!({"token": "abc"}))]).await;
    assert!(report.outcome.failure().is_none(), "{:?}", report.outcome);
    assert_eq!(report.saved.get("token"), Some(&json!("abc")));
    assert_eq!(report.response.as_deref().and_then(|response| response.get("status")), Some(&json!(201)));
    let request = &sent[0].request;
    assert_eq!((request.method.as_str(), request.uri.to_string()), ("POST", "http://id.test/login?as=ada".into()));
    assert_eq!(request.headers.get("x-step").unwrap(), "ada");
    assert_eq!(request.headers.get("x-trace").unwrap(), "t");
    assert_eq!(request.headers.get(CONTENT_TYPE).unwrap(), "text/plain; charset=utf-8");
    assert_eq!(request.body, Bytes::from("hi ada"));
    assert_eq!(report.checks.len(), 1);
    assert_eq!((report.checks[0].kind, &report.checks[0].origin), (CheckKind::Status, &Origin::Action(action)));
}

#[tokio::test]
async fn an_inline_request_fails_on_a_status_other_than_2xx_and_saves_nothing() {
    let mut fixture = Fixture::new();
    let action = inline(&mut fixture, "http://id.test/login");
    fixture.action_mut(action).save.values = fixture.vars(&[("token", "${response.body.token}")]);
    let (report, _) = send(fixture, action, vec![json_response(500, &json!({"token": "abc"}))]).await;
    assert_eq!(report.outcome.failure().and_then(|failure| failure.action), Some(action));
    assert!(report.saved.is_empty());
}

#[tokio::test]
async fn an_inline_url_that_does_not_parse_fails_before_sending() {
    let mut fixture = Fixture::new();
    let action = inline(&mut fixture, "http://bad host/");
    let (report, sent) = send(fixture, action, vec![]).await;
    assert!(report.outcome.failure().is_some_and(|failure| failure.message.contains("is not a URL")));
    assert!(sent.is_empty());
}

#[tokio::test]
async fn the_inline_executor_runs_only_inline_actions_and_follows_nothing() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let parts = PipelineParts::new(Arc::new(FakeTransport::scripted(Vec::new())), HostLimits::spawn(1));
    let executor = super::InlineExecutor::new(parts);
    let action = fixture.program.actions[0].clone();
    assert!(crate::ActionExecutor::follows(&executor, &fixture.program, &action).is_empty());
    let cx = fixture.cx(ActionExecutors::new());
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx, frame: Frame::default() };
    let report = crate::ActionExecutor::execute(&executor, request).await;
    assert!(report.outcome.failure().is_some_and(|failure| failure.message.contains("not inline")));
}
