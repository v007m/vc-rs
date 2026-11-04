use std::sync::Arc;

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_model::{ActionId, ActionKind, BodySpec, HttpResponse, Method, OwnerKey, Path, Segment, TemplateValue};
use vc_testkit::{FakeTransport, Recorded, RecordingTransport, status};

use crate::fixture::{Fixture, literal};
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PendingCleanup, PipelineParts, Saved};

fn json_response(code: u16, body: &Value) -> HttpResponse {
    let body = Bytes::from(body.to_string());
    let headers = HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_static("application/json"))]);
    HttpResponse { status: StatusCode::from_u16(code).unwrap(), headers, wire_size: body.len() as u64, body }
}

/// Runs `action` in `frame` against a transport answering `answers` in turn, recording what
/// was sent.
async fn send(
    fixture: Fixture,
    action: ActionId,
    frame: Frame,
    answers: Vec<HttpResponse>,
) -> (ActionReport, Vec<Recorded>) {
    let fake = FakeTransport::scripted(answers.into_iter().map(Ok).collect());
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame };
    let report = cx.executors.execute(request).await;
    (report, recordings.take())
}

fn code(report: &ActionReport) -> Option<String> {
    report.outcome.failure().and_then(|failure| failure.code.clone())
}

fn saved(value: Value) -> Saved {
    let Value::Object(map) = value else { unreachable!("an object") };
    map
}

#[tokio::test]
async fn the_request_is_rendered_from_every_level_of_the_scope() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let mut vars = fixture.vars(&[("userId", "u-1"), ("email", "e@x"), ("greeting", "a"), ("name", "b c")]);
    vars.extend([(fixture.var("page"), TemplateValue::Number(2.into())), (fixture.var("region"), TemplateValue::Null)]);
    fixture.program.config.vars.values = vars;
    fixture.program.config.defaults.headers = fixture.templates(&[("x-trace", "t-${var.page}"), ("x-api", "config")]);
    fixture.group_mut().base_url = Some(fixture.template("http://api.test/v1"));
    fixture.group_mut().headers = fixture.templates(&[("accept", "application/json")]);
    let api_query =
        fixture.templates(&[("page", "${var.page}"), ("tags", "${var.greeting}"), ("region", "${var.region}")]);
    let api_headers = fixture.templates(&[("x-api", "api"), ("accept", "${var.region}")]);
    let body = TemplateValue::Map([("email".to_owned(), fixture.value("${var.email}"))].into());
    let path = fixture.template("/users/${var.userId}");
    let api = fixture.api_mut(call);
    (api.method, api.path, api.query, api.headers, api.body) =
        (Method::Post, path, api_query, api_headers, BodySpec::Json(body));
    let action_query = fixture.templates(&[("page", "3"), ("q", "${var.name}")]);
    let action_headers = fixture.templates(&[("x-api", "action")]);
    (fixture.action_mut(call).query, fixture.action_mut(call).headers) = (action_query, action_headers);
    let (report, sent) = send(fixture, call, Frame::default(), vec![status(201)]).await;
    assert_eq!(report.outcome.failure(), None);
    let request = &sent[0].request;
    assert_eq!(
        (request.method.as_str(), request.uri.to_string()),
        ("POST", "http://api.test/v1/users/u-1?page=3&tags=a&q=b%20c".to_owned())
    );
    let headers: Vec<_> =
        request.headers.iter().map(|(name, value)| (name.as_str(), value.to_str().unwrap())).collect();
    assert_eq!(headers, [("x-trace", "t-2"), ("x-api", "action"), ("content-type", "application/json")]);
    assert_eq!(request.body, Bytes::from(r#"{"email":"e@x"}"#));
}

#[tokio::test]
async fn a_list_repeats_its_query_key_and_unreserved_characters_stay() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users?fixed=1");
    let tags = TemplateValue::List(vec![fixture.value("a-b_c.d~e"), TemplateValue::Null, fixture.value("x&y=z")]);
    fixture.action_mut(call).vars.values = [(fixture.var("greeting"), tags)].into();
    fixture.api_mut(call).query = fixture.templates(&[("tag s", "${var.greeting}")]);
    let (_, sent) = send(fixture, call, Frame::default(), vec![status(200)]).await;
    let uri = sent[0].request.uri.to_string();
    assert_eq!(uri, "http://api.test/users?fixed=1&tag%20s=a-b_c.d~e&tag%20s=x%26y%3Dz");
}

/// What a `GET /body` with `body` and `headers` sent: its body and `content-type`, or the
/// failure code when nothing was sent.
async fn sent_body(
    body: impl FnOnce(&mut Fixture) -> BodySpec,
    headers: &[(&str, &str)],
) -> Result<(Bytes, String), Option<String>> {
    let mut fixture = Fixture::new();
    let call = fixture.call("/body");
    let body = body(&mut fixture);
    fixture.api_mut(call).headers = fixture.templates(headers);
    fixture.api_mut(call).body = body;
    let (report, sent) = send(fixture, call, Frame::default(), vec![status(200)]).await;
    let recorded = sent.first().ok_or_else(|| code(&report))?;
    let kind = recorded.request.headers.get(CONTENT_TYPE).and_then(|kind| kind.to_str().ok()).unwrap_or_default();
    Ok((recorded.request.body.clone(), kind.to_owned()))
}

fn object(pairs: &[(&str, i32)]) -> TemplateValue {
    TemplateValue::Map(pairs.iter().map(|(key, n)| ((*key).to_owned(), TemplateValue::Number((*n).into()))).collect())
}

#[tokio::test]
async fn each_body_kind_sets_its_content_type_unless_one_is_given() {
    let text =
        sent_body(|fixture| BodySpec::Text(fixture.template("hi ${var.greeting | default('there')}")), &[]).await;
    assert_eq!(text, Ok((Bytes::from("hi there"), "text/plain; charset=utf-8".to_owned())));
    let bytes = sent_body(|_| BodySpec::Base64(literal(" aGk= ")), &[("content-type", "image/png")]).await;
    assert_eq!(bytes, Ok((Bytes::from("hi"), "image/png".to_owned())));
    let raw = sent_body(|_| BodySpec::Base64(literal("aGk=")), &[]).await;
    assert_eq!(raw, Ok((Bytes::from("hi"), "application/octet-stream".to_owned())));
    let extended = sent_body(
        |fixture| {
            fixture.program.config.vars.values = [(fixture.var("token"), object(&[("a", 1), ("c", 3)]))].into();
            let remove = vec![Path { segments: vec![Segment::Key("c".to_owned())] }];
            BodySpec::Extend { base: fixture.template("${var.token}"), changes: object(&[("b", 2)]), remove }
        },
        &[],
    )
    .await;
    assert_eq!(extended, Ok((Bytes::from(r#"{"a":1,"b":2}"#), "application/json".to_owned())));
}

#[tokio::test]
async fn bodies_and_headers_that_cannot_be_built_fail_before_sending() {
    let v5111 = Err(Some(Code::V5111.to_string()));
    assert_eq!(sent_body(|_| BodySpec::Base64(literal("not base64!")), &[]).await, v5111);
    assert_eq!(sent_body(|_| BodySpec::File(literal("avatar.png")), &[]).await, v5111);
    assert_eq!(sent_body(|_| BodySpec::Form(IndexMap::new()), &[]).await, v5111);
    assert_eq!(sent_body(|_| BodySpec::Multipart(IndexMap::new()), &[]).await, v5111);
    assert_eq!(sent_body(|_| BodySpec::None, &[("x-bad", "line\nbreak")]).await, v5111);
    assert_eq!(sent_body(|_| BodySpec::None, &[("bad name", "x")]).await, v5111);
}

#[tokio::test]
async fn saves_run_the_apis_then_the_actions_against_the_response_and_request() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/users");
    let body = TemplateValue::Map([("email".to_owned(), fixture.value("e@x"))].into());
    fixture.api_mut(call).body = BodySpec::Json(body);
    let api_saves = fixture.vars(&[("userId", "${response.body.id}"), ("email", "${request.body.email}")]);
    fixture.api_mut(call).save.values = api_saves;
    let action_saves = fixture.vars(&[("userId", "${response.body.id}-mine"), ("name", "${response.status}")]);
    fixture.action_mut(call).save.values = action_saves;
    let (report, _) = send(fixture, call, Frame::default(), vec![json_response(201, &json!({ "id": 7 }))]).await;
    assert_eq!(report.outcome.failure(), None);
    assert_eq!(report.saved, saved(json!({ "userId": "7-mine", "email": "e@x", "name": 201 })));
    let timing = json!({ "total": 0.0, "firstByte": 0.0, "download": 0.0 });
    let headers = json!({ "content-type": "application/json" });
    let response = json!({ "status": 201, "headers": headers, "body": { "id": 7 }, "timing": timing });
    assert_eq!(report.response.as_deref(), Some(&response));
}

/// A `POST /users` whose api's cleanup is an action with `cleanup_vars`.
fn creating(cleanup_vars: &[(&str, &str)]) -> (Fixture, ActionId, ActionId) {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users");
    fixture.action_mut(delete).vars.values = fixture.vars(cleanup_vars);
    fixture.program.config.vars.values = fixture.vars(&[("greeting", "hello")]);
    let create = fixture.creating("/users", delete);
    (fixture, create, delete)
}

#[tokio::test]
async fn the_cleanup_is_registered_with_its_vars_rendered_against_the_response() {
    let (fixture, create, delete) = creating(&[("userId", "${response.body.id}"), ("name", "${var.greeting}")]);
    let (report, _) = send(fixture, create, Frame::default(), vec![json_response(201, &json!({ "id": 7 }))]).await;
    let vars = saved(json!({ "userId": 7, "name": "hello" }));
    assert_eq!(report.cleanups, vec![PendingCleanup::new(delete).with_vars(vars)]);
}

#[tokio::test]
async fn a_call_run_as_a_cleanup_registers_none() {
    let (fixture, create, _) = creating(&[]);
    let frame = Frame::default().cleaning(&Arc::new(Saved::new()));
    let (report, _) = send(fixture, create, frame, vec![status(201)]).await;
    assert_eq!((report.outcome.failure(), report.cleanups), (None, vec![]));
}

#[tokio::test]
async fn a_failed_save_keeps_the_cleanup_and_a_failed_cleanup_var_drops_it() {
    let (mut fixture, create, delete) = creating(&[]);
    fixture.action_mut(create).save.values = fixture.vars(&[("userId", "${response.body.missing}")]);
    let (report, _) = send(fixture, create, Frame::default(), vec![json_response(201, &json!({}))]).await;
    assert_eq!(code(&report), Some(Code::V5111.to_string()));
    assert_eq!(report.cleanups, vec![PendingCleanup::new(delete)], "what was created is still cleaned up");
    let (fixture, create, _) = creating(&[("userId", "${response.body.missing}")]);
    let (report, _) = send(fixture, create, Frame::default(), vec![json_response(201, &json!({}))]).await;
    assert_eq!((code(&report), report.cleanups), (Some(Code::V5111.to_string()), vec![]));
}

#[tokio::test]
async fn a_reuse_runs_its_steps_with_their_saves_step_ids_and_its_callers_vars() {
    let mut fixture = Fixture::new();
    let create = fixture.call("/users");
    fixture.action_mut(create).id = Some(fixture.sym("created"));
    fixture.action_mut(create).save.values = fixture.vars(&[("userId", "${response.body.id}")]);
    let get = fixture.call("/users");
    fixture.api_mut(get).path = fixture.template("/users/${var.userId}/${created.body.id}/${var.region}/${var.name}");
    let reused = fixture.test(vec![create, get], None);
    fixture.test_mut(reused).vars.values = fixture.vars(&[("region", "test"), ("name", "n")]);
    let reuse = fixture.action(ActionKind::Test(reused));
    fixture.action_mut(reuse).vars.values = fixture.vars(&[("region", "eu")]);
    let answers = vec![json_response(201, &json!({ "id": 7 })), json_response(200, &json!({ "ok": true }))];
    let (report, sent) = send(fixture, reuse, Frame::default(), answers).await;
    assert_eq!(report.outcome.failure(), None);
    assert_eq!(sent.get(1).map(|recorded| recorded.request.uri.path()), Some("/users/7/7/eu/n"));
    assert_eq!(report.saved, saved(json!({ "userId": 7 })));
    assert_eq!(report.response.map(|response| response["body"].clone()), Some(json!({ "ok": true })));
}
