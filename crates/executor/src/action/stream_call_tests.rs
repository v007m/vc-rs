use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use http::HeaderValue;
use http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use serde_json::{Value, json};
use vc_interfaces::{
    HookCall, HookOutput, MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport,
    TransportError,
};
use vc_model::{
    ActionId, ExchangeItem, ExchangeKind, HookRef, HttpResponse, Matcher, MatcherArg, MatcherCall, Origin, OwnerKey,
    Protocol, RenderedRequest, StreamSettings, TemplateValue,
};
use vc_testkit::{FakeHooks, FakeStreamTransport, FakeTransport, StreamAnswer};

use crate::fixture::{self, Fixture};
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PendingCleanup, PipelineParts};

/// Parts that open streams through `streams`.
fn parts(streams: FakeStreamTransport) -> PipelineParts {
    let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]);
    PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)).with_streams(Arc::new(streams))
}

async fn run(fixture: Fixture, action: ActionId, parts: PipelineParts) -> ActionReport {
    let cx = fixture.cx(ActionExecutors::standard(parts));
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

/// A call of a new `protocol` api at `path` whose exchange is `items`.
fn streaming(fixture: &mut Fixture, (protocol, path): (Protocol, &str), items: Vec<ExchangeItem>) -> ActionId {
    let call = fixture.call(path);
    let api = fixture.api_mut(call);
    (api.protocol, api.exchange) = (protocol, items);
    call
}

/// A message check of these fields, each equal to its text, as the compiler lowers it (D379).
fn check(fields: &[(&str, &str)]) -> Matcher {
    let equals = |text: &str| Matcher::Equals(TemplateValue::String(fixture::literal(text)));
    let fields = fields.iter().map(|(name, text)| ((*name).to_owned(), equals(text))).collect();
    Matcher::All(vec![MatcherCall {
        name: "fields".to_owned(),
        arg: MatcherArg::Fields(fields),
        span: fixture::span(),
    }])
}

fn item(kind: ExchangeKind) -> ExchangeItem {
    ExchangeItem { kind, span: fixture::span() }
}

fn receive(fields: &[(&str, &str)]) -> ExchangeItem {
    item(ExchangeKind::Receive(check(fields)))
}

fn send(text: &str) -> ExchangeItem {
    item(ExchangeKind::Send(TemplateValue::String(fixture::literal(text))))
}

/// A 200 `text/event-stream` head.
fn events_head() -> HttpResponse {
    let mut head = vc_testkit::status(200);
    drop(head.headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream")));
    head
}

fn chunk(text: &str) -> StreamFrame {
    StreamFrame::Chunk(Bytes::from(text.to_owned()))
}

/// An event stream of `chunks`, then its end.
fn events(chunks: &[&str]) -> StreamAnswer {
    StreamAnswer::stream(events_head(), chunks.iter().map(|text| Ok(chunk(text))).collect())
}

/// A check's code and error.
type Coded = (Option<String>, Option<String>);

/// The messages checks of `report`: each one's code and error.
fn messages_checks(report: &ActionReport) -> Vec<Coded> {
    let checks = report.checks.iter().filter(|check| check.kind == vc_model::CheckKind::Messages);
    checks.map(|check| (check.code.clone(), check.error.clone())).collect()
}

fn root(report: &ActionReport, key: &str) -> Value {
    report.response.as_ref().map(|response| response[key].clone()).unwrap_or_default()
}

/// An `sse` call asks for an event stream, reads its events as messages under
/// `response.messages`, and fills `response.exchange` with the message each `receive` matched.
#[tokio::test]
async fn an_sse_call_asks_for_an_event_stream_and_reads_its_events_as_messages() {
    let mut fixture = Fixture::new();
    let call = streaming(&mut fixture, (Protocol::Sse, "/prices"), vec![receive(&[("event", "price")])]);
    let streams = FakeStreamTransport::new(|request, mode, _| {
        assert_eq!(mode, StreamMode::Body);
        let asked = (request.headers.get(ACCEPT), request.headers.get(CACHE_CONTROL));
        assert_eq!(
            asked,
            (Some(&HeaderValue::from_static("text/event-stream")), Some(&HeaderValue::from_static("no-cache")))
        );
        Ok(events(&[": hello\nevent: tick\ndata: 0\n\nevent: price\nda", "ta: {\"p\":1}\nid: 7\n\n", "data: late\n\n"]))
    });
    let report = run(fixture, call, parts(streams)).await;
    assert_eq!(report.outcome.failure(), None, "{:?}", report.checks);
    let tick = json!({"event": "tick", "data": 0, "text": "0"});
    let price = json!({"event": "price", "data": {"p": 1}, "id": "7", "text": "{\"p\":1}"});
    assert_eq!(root(&report, "messages"), json!([tick, price]), "the exchange is done after the price");
    assert_eq!(root(&report, "exchange"), json!([price]));
    assert_eq!(report.exchanges.first().map(|exchange| exchange.messages.len()), Some(2));
}

/// A `websocket` call sends each `send` as it is reached, a `receive` waits for its message,
/// and `response.exchange` holds what was sent and what was received, in order.
#[tokio::test]
async fn a_websocket_call_sends_and_receives_in_the_order_written() {
    let mut fixture = Fixture::new();
    let items =
        vec![send("hi"), receive(&[("type", "text"), ("text", "hi")]), send("bye"), receive(&[("type", "close")])];
    let call = streaming(&mut fixture, (Protocol::Websocket, "/ws"), items);
    let report = run(fixture, call, parts(FakeStreamTransport::contract())).await;
    assert_eq!(report.outcome.failure(), None, "{:?}", report.checks);
    let echoed = json!({"type": "text", "data": "hi", "text": "hi"});
    let closed = json!({"type": "close", "code": 1000, "reason": "bye"});
    assert_eq!(root(&report, "exchange"), json!(["hi", echoed, "bye", closed]));
    assert_eq!(root(&report, "messages"), json!([echoed, closed]));
}

/// A group pairs its checks with distinct messages in any order; its item holds the messages
/// in the order its checks were written.
#[tokio::test]
async fn a_group_receive_pairs_its_checks_with_messages_in_any_order() {
    let mut fixture = Fixture::new();
    let group = item(ExchangeKind::Group(vec![check(&[("event", "b")]), check(&[("event", "a")])]));
    let call = streaming(&mut fixture, (Protocol::Sse, "/feed"), vec![group]);
    let streams = FakeStreamTransport::scripted(vec![Ok(events(&["event: a\ndata: 1\n\nevent: b\ndata: 2\n\n"]))]);
    let report = run(fixture, call, parts(streams)).await;
    assert_eq!(report.outcome.failure(), None, "{:?}", report.checks);
    let (a, b) = (json!({"event": "a", "data": 1, "text": "1"}), json!({"event": "b", "data": 2, "text": "2"}));
    assert_eq!(root(&report, "exchange"), json!([[b, a]]));
}

/// A `receive` still waiting when the stream closes, or when the call's time runs out, is V5130
/// at the item, with what it expected and the messages since the last match.
#[tokio::test(start_paused = true)]
async fn an_unmet_receive_is_v5130_when_the_stream_closes_or_the_time_runs_out() {
    let mut fixture = Fixture::new();
    let call = streaming(&mut fixture, (Protocol::Sse, "/prices"), vec![receive(&[("event", "quote")])]);
    let api = Origin::Api(fixture.api_mut(call).id);
    let streams = FakeStreamTransport::scripted(vec![Ok(events(&["event: price\ndata: 1\n\n"]))]);
    let report = run(fixture, call, parts(streams)).await;
    let error = "no message matched before the stream closed".to_owned();
    assert_eq!(messages_checks(&report), vec![(Some("V5130".to_owned()), Some(error))]);
    let failed = report.checks.iter().find(|check| check.code.is_some());
    let actual = failed.map(|check| (check.origin, check.actual.clone()));
    assert_eq!(actual, Some((api, Some(json!([{"event": "price", "data": 1, "text": "1"}])))));

    let mut fixture = Fixture::new();
    let call = streaming(&mut fixture, (Protocol::Websocket, "/ws"), vec![receive(&[("type", "text")])]);
    fixture.api_mut(call).timeout = Some(vc_model::Duration::new(Duration::from_secs(2)));
    let report = run(fixture, call, parts(FakeStreamTransport::contract())).await;
    let error = "no message matched before the 2s time limit passed".to_owned();
    assert_eq!(messages_checks(&report), vec![(Some("V5130".to_owned()), Some(error))]);
}

/// A stream that did not open is V5131: at the first `receive`, else at the step when the
/// opening status succeeded (a failed one already fails the status check).
#[tokio::test]
async fn a_stream_that_did_not_open_is_v5131_at_its_first_receive_else_at_the_step() {
    let mut json_head = vc_testkit::status(200);
    drop(json_head.headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json")));
    let refused = "the stream did not open: the server answered 404";
    let not_events = "the stream did not open: the response is `application/json`, not `text/event-stream`";
    let cases = [
        (StreamAnswer::refused(vc_testkit::status(404)), true, Some(refused)),
        (StreamAnswer::refused(vc_testkit::status(404)), false, None),
        (StreamAnswer::stream(json_head.clone(), vec![Ok(chunk("data: 1\n\n"))]), true, Some(not_events)),
        (StreamAnswer::stream(json_head, vec![]), false, Some(not_events)),
    ];
    for (answer, receives, error) in cases {
        let mut fixture = Fixture::new();
        let items = if receives { vec![receive(&[("event", "message")])] } else { vec![] };
        let call = streaming(&mut fixture, (Protocol::Sse, "/prices"), items);
        let origin = if receives { Origin::Api(fixture.api_mut(call).id) } else { Origin::Action(call) };
        let report = run(fixture, call, parts(FakeStreamTransport::scripted(vec![Ok(answer)]))).await;
        let failed: Vec<_> = report.checks.iter().filter(|check| check.code.as_deref() == Some("V5131")).collect();
        let found: Vec<_> = failed.iter().map(|check| (check.origin, check.error.as_deref())).collect();
        let wanted: Vec<_> = error.map(|error| (origin, Some(error))).into_iter().collect();
        assert_eq!(found, wanted, "receives: {receives}");
        assert!(report.outcome.failure().is_some(), "the call fails either way");
    }
}

/// `sse.reconnect` opens an ended stream once more, after the server's `retry`, sending the
/// last `id` as `last-event-id`; it does not when that wait passes the call's time limit, nor
/// when `reconnect` is off.
#[tokio::test(start_paused = true)]
async fn a_reconnecting_sse_call_resumes_from_the_last_id_within_its_time_limit() {
    for (reconnect, timeout, opens) in [(true, 30, 2), (true, 1, 1), (false, 30, 1)] {
        let mut fixture = Fixture::new();
        let call = streaming(&mut fixture, (Protocol::Sse, "/feed"), vec![receive(&[("text", "3")])]);
        let api = fixture.api_mut(call);
        (api.stream, api.timeout) =
            (Some(StreamSettings { reconnect }), Some(vc_model::Duration::new(Duration::from_secs(timeout))));
        let streams = FakeStreamTransport::new(|request, _, open| {
            let last = request.headers.get("last-event-id").and_then(|id| id.to_str().ok());
            assert_eq!(last, (open == 1).then_some("2"), "open {open}");
            let dropped = Err(TransportError::Protocol { reason: "reset".to_owned() });
            let first = vec![Ok(chunk("retry: 2000\nid: 1\ndata: 1\n\nid: 2\ndata: 2\n\n")), dropped];
            Ok(if open == 0 { StreamAnswer::stream(events_head(), first) } else { events(&["id: 3\ndata: 3\n\n"]) })
        });
        let report = run(fixture, call, parts(streams.clone())).await;
        assert_eq!(streams.calls(), opens, "reconnect {reconnect}, timeout {timeout}s");
        let failed = messages_checks(&report).into_iter().find_map(|(code, _)| code);
        assert_eq!(failed.as_deref(), (opens == 1).then_some("V5130"));
        let texts = if opens == 2 { json!(["1", "2", "3"]) } else { json!(["1", "2"]) };
        let messages = root(&report, "messages");
        let read: Vec<_> = messages.as_array().into_iter().flatten().map(|message| &message["text"]).collect();
        assert_eq!(json!(read), texts);
    }
}

/// A stream whose frames pass the body limit fails the call with V5105, a failed `messages`
/// check that keeps the messages read before it (D425).
#[tokio::test]
async fn a_stream_larger_than_the_body_limit_fails_with_v5105() {
    let mut fixture = Fixture::new();
    let call = streaming(&mut fixture, (Protocol::Sse, "/feed"), vec![receive(&[("event", "never")])]);
    let limit = vc_interfaces::SendContext::DEFAULT_BODY_LIMIT.bytes();
    let big = vec![b'a'; usize::try_from(limit).unwrap() + 1];
    let streams = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::stream(
        events_head(),
        vec![Ok(chunk("data: 1\n\n")), Ok(StreamFrame::Chunk(Bytes::from(big)))],
    ))]);
    let report = run(fixture, call, parts(streams)).await;
    let failure = report.outcome.failure().map(|failure| (failure.code.clone(), failure.message.clone()));
    let message = format!("messages: the stream's messages are larger than {limit} bytes");
    assert_eq!(failure, Some((Some("V5105".to_owned()), message)));
    let kept: Vec<_> = report.exchanges.iter().map(|exchange| exchange.messages.len()).collect();
    assert_eq!(kept, [1], "the call reports the messages read before the limit");
}

/// A reconnect that answers with no event stream ends the read: the waiting `receive` is V5130
/// naming the answer. One that fails as a first open would is a failed `messages` check under its
/// error. Either way the messages of the first connection are kept (D425).
#[tokio::test(start_paused = true)]
async fn a_failed_reconnect_is_v5130_with_its_answer_or_a_failed_check_under_its_error() {
    let mut json_head = vc_testkit::status(200);
    drop(json_head.headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json")));
    let not_events = "the response is `application/json`, not `text/event-stream`";
    let broken = Err(TransportError::Connect { host: "api.test".to_owned(), reason: "refused".to_owned() });
    for (second, code, reason) in [
        (Ok(StreamAnswer::refused(vc_testkit::status(500))), "V5130", Some("the server answered 500")),
        (Ok(StreamAnswer::stream(json_head.clone(), vec![])), "V5130", Some(not_events)),
        (broken, "V5105", None),
    ] {
        let mut fixture = Fixture::new();
        let call = streaming(&mut fixture, (Protocol::Sse, "/feed"), vec![receive(&[("text", "3")])]);
        fixture.api_mut(call).stream = Some(StreamSettings { reconnect: true });
        let streams = FakeStreamTransport::scripted(vec![Ok(events(&["id: 1\ndata: 1\n\n"])), second]);
        let report = run(fixture, call, parts(streams)).await;
        let checks = messages_checks(&report);
        let (found, error) = checks.first().cloned().unwrap_or_default();
        assert_eq!((checks.len(), found.as_deref()), (1, Some(code)), "{checks:?}");
        if let Some(reason) = reason {
            assert_eq!(error, Some(format!("no message matched before the reconnect failed ({reason})")));
        }
        let kept: Vec<_> = report.exchanges.iter().map(|exchange| exchange.messages.len()).collect();
        assert_eq!(kept, [1], "{code}: the first connection's message is kept");
    }
}

/// Where a [`Stuck`] transport does not answer as asked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hang {
    /// A `send` on any of its streams.
    Sends,
    /// Every open after the first.
    Reopens,
    /// A `send` on any of its streams, which fails at once with [`reset`].
    Breaks,
}

/// A stream transport that hangs or fails where its [`Hang`] says.
struct Stuck(FakeStreamTransport, Hang);

struct StuckSend(Box<dyn MessageStream>, Hang);

#[async_trait]
impl StreamTransport for Stuck {
    async fn open(
        &self,
        request: RenderedRequest,
        mode: StreamMode,
        ctx: &SendContext,
    ) -> Result<Opened, TransportError> {
        if self.1 == Hang::Reopens && self.0.calls() > 0 {
            return std::future::pending().await;
        }
        let opened = self.0.open(request, mode, ctx).await?;
        let stuck = |stream| Box::new(StuckSend(stream, self.1)) as Box<dyn MessageStream>;
        let stream = if self.1 == Hang::Reopens { opened.stream } else { opened.stream.map(stuck) };
        Ok(Opened { stream, ..opened })
    }
}

#[async_trait]
impl MessageStream for StuckSend {
    async fn send(&mut self, _frame: OutgoingFrame) -> Result<(), TransportError> {
        if self.1 == Hang::Breaks {
            return Err(reset());
        }
        std::future::pending().await
    }

    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>> {
        self.0.next().await
    }

    async fn close(&mut self, code: u16) -> Result<(), TransportError> {
        self.0.close(code).await
    }
}

/// Parts whose stream port is `streams`, hanging where `hang` says.
fn stuck(streams: FakeStreamTransport, hang: Hang) -> PipelineParts {
    let fake = FakeTransport::scripted(vec![]);
    PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)).with_streams(Arc::new(Stuck(streams, hang)))
}

/// A reconnect that does not answer within the call's time limit leaves the `receive` V5130,
/// naming the limit (D425).
#[tokio::test(start_paused = true)]
async fn a_reconnect_that_does_not_answer_in_time_is_v5130_naming_the_limit() {
    let mut fixture = Fixture::new();
    let call = streaming(&mut fixture, (Protocol::Sse, "/feed"), vec![receive(&[("text", "3")])]);
    let api = fixture.api_mut(call);
    (api.stream, api.timeout) =
        (Some(StreamSettings { reconnect: true }), Some(vc_model::Duration::new(Duration::from_secs(5))));
    let streams = FakeStreamTransport::scripted(vec![Ok(events(&["id: 1\ndata: 1\n\n"]))]);
    let report = run(fixture, call, stuck(streams, Hang::Reopens)).await;
    let error = "no message matched before the 5s time limit passed".to_owned();
    assert_eq!(messages_checks(&report), [(Some("V5130".to_owned()), Some(error))]);
}

/// The error a [`Hang::Breaks`] stream's `send` fails with.
fn reset() -> TransportError {
    TransportError::Connect { host: "api.test".to_owned(), reason: "reset".to_owned() }
}

/// A `send` still going out at the call's time limit is a failed `messages` check, V5104, and
/// one the transport refuses is a failed `messages` check under its error's code, V5105;
/// neither is reported as sent, and the opened api's cleanup still registers on its `101`
/// (D425).
#[tokio::test(start_paused = true)]
async fn a_send_that_does_not_go_out_is_a_failed_check_and_the_cleanup_still_registers() {
    for (hang, code, error) in [
        (Hang::Sends, "V5104", "a `send` did not go out within the 2s time limit".to_owned()),
        (Hang::Breaks, "V5105", reset().to_string()),
    ] {
        let mut fixture = Fixture::new();
        let delete = fixture.call("/rooms");
        let call = fixture.creating("/ws", delete);
        let api = fixture.api_mut(call);
        (api.protocol, api.exchange) = (Protocol::Websocket, vec![send("hi")]);
        api.timeout = Some(vc_model::Duration::new(Duration::from_secs(2)));
        let report = run(fixture, call, stuck(FakeStreamTransport::contract(), hang)).await;
        let failure = report.outcome.failure().map(|failure| (failure.code.clone(), failure.message.clone()));
        assert_eq!(failure, Some((Some(code.to_owned()), format!("messages: {error}"))));
        assert_eq!(
            root(&report, "exchange"),
            json!([null]),
            "{code}: a send that did not go out is not reported as sent"
        );
        assert_eq!(report.cleanups, [PendingCleanup::new(delete)]);
    }
}

/// `afterResponse` hooks run once the exchange is done, and see its messages and items (D423).
#[tokio::test]
async fn after_response_hooks_see_the_messages_and_the_exchange() {
    let mut fixture = Fixture::new();
    let call = streaming(&mut fixture, (Protocol::Websocket, "/ws"), vec![send("hi"), receive(&[("text", "hi")])]);
    fixture.api_mut(call).hooks.after_response = vec![HookRef { file: "hooks/log.ts".into(), export: "log".into() }];
    let hooks = FakeHooks::new().on("hooks/log.ts#log", |call: &HookCall| {
        let response = &call.input["response"];
        assert_eq!((&response["status"], &response["messages"][0]["text"]), (&json!(101), &json!("hi")));
        assert_eq!(response["exchange"][0], json!("hi"));
        Ok(HookOutput::new(Value::Null))
    });
    let parts = parts(FakeStreamTransport::contract()).with_hook_runner(Arc::new(hooks));
    let report = run(fixture, call, parts).await;
    assert_eq!(report.outcome.failure(), None, "{:?}", report.checks);
}
