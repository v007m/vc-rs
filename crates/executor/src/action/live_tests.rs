//! A live stream opens as a stream call does, then reads and sends for as long as its caller
//! asks, and says how it ended (D543).

use std::sync::Arc;

use bytes::Bytes;
use http::HeaderValue;
use http::header::CONTENT_TYPE;
use serde_json::{Value, json};
use vc_interfaces::StreamFrame;
use vc_model::{ActionId, Protocol, StreamMessage};
use vc_testkit::{FakeStreamTransport, FakeTransport, StreamAnswer};

use super::{LiveEnd, LiveRead, LiveStream};
use crate::fixture::Fixture;
use crate::{ActionExecutors, ActionRequest, HostLimits, PipelineParts};

fn parts(streams: FakeStreamTransport) -> PipelineParts {
    let fake = FakeTransport::scripted(vec![Ok(vc_testkit::status(200))]);
    PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)).with_streams(Arc::new(streams))
}

/// A live stream of a new `protocol` api at `path`, opened through `streams`.
async fn opened(protocol: Protocol, path: &str, streams: FakeStreamTransport) -> Result<LiveStream, String> {
    let mut fixture = Fixture::new();
    let call: ActionId = fixture.call(path);
    fixture.api_mut(call).protocol = protocol;
    let cx = fixture.cx(ActionExecutors::standard(parts(streams)));
    let live = cx.executors.open_live(ActionRequest::run(&cx, call)).await;
    live.map_err(|failure| failure.message)
}

fn fields(read: &LiveRead) -> Value {
    match read {
        LiveRead::Message(StreamMessage { fields }) => Value::Object(fields.clone()),
        LiveRead::Ended(end) => json!({"ended": end.reason, "failed": end.failed, "code": end.code}),
    }
}

#[tokio::test]
async fn a_live_websocket_echoes_what_is_sent_until_the_server_closes_it() {
    let mut live = opened(Protocol::Websocket, "/ws", FakeStreamTransport::contract()).await.unwrap();
    assert_eq!(live.head().response.status.as_u16(), 101);
    live.send(&json!("hi")).await.unwrap();
    assert_eq!(fields(&live.next().await), json!({"type": "text", "data": "hi", "text": "hi"}));
    live.send(&json!({"n": 1})).await.unwrap();
    assert_eq!(fields(&live.next().await)["data"], json!({"n": 1}), "a value goes out as JSON text");
    live.send(&json!("bye")).await.unwrap();
    assert_eq!(fields(&live.next().await), json!({"type": "close", "code": 1000, "reason": "bye"}));
    let ended = LiveEnd { code: Some(1000), ..LiveEnd::new("the stream closed", false) };
    assert_eq!(live.next().await, LiveRead::Ended(ended.clone()));
    assert_eq!(live.next().await, LiveRead::Ended(ended), "every later read ends it again");
    assert!(live.send(&json!("late")).await.is_err(), "nothing goes out once it ended");
}

#[tokio::test]
async fn a_live_websocket_closed_by_its_caller_ends() {
    let mut live = opened(Protocol::Websocket, "/ws", FakeStreamTransport::contract()).await.unwrap();
    live.close(1000).await.unwrap();
    let LiveRead::Ended(end) = live.next().await else { panic!("ended") };
    assert!(!end.failed, "{end:?}");
}

#[tokio::test]
async fn a_live_event_stream_reads_its_events_and_cannot_send() {
    let mut head = vc_testkit::status(200);
    drop(head.headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream")));
    let chunk = StreamFrame::Chunk(Bytes::from_static(b"event: tick\ndata: 1\n\nevent: tick\ndata: 2\n\n"));
    let streams = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::stream(head, vec![Ok(chunk)]))]);
    let mut live = opened(Protocol::Sse, "/ticks", streams).await.unwrap();
    assert!(live.send(&json!("hi")).await.is_err(), "an event stream only reads");
    assert_eq!(fields(&live.next().await), json!({"event": "tick", "data": 1, "text": "1"}));
    assert_eq!(fields(&live.next().await)["data"], json!(2), "one chunk can carry two events");
    assert_eq!(live.next().await, LiveRead::Ended(LiveEnd::new("the stream closed", false)));
}

#[tokio::test]
async fn an_answer_that_opens_no_stream_ends_it_failed_at_the_first_read() {
    let streams = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::refused(vc_testkit::status(401)))]);
    let mut live = opened(Protocol::Websocket, "/ws", streams).await.unwrap();
    assert_eq!(live.head().response.status.as_u16(), 401, "the head is still shown");
    let LiveRead::Ended(end) = live.next().await else { panic!("ended") };
    assert!(end.failed && end.reason.contains("did not open: the server answered 401"), "{end:?}");
}

#[tokio::test]
async fn only_a_streaming_api_opens_a_live_stream() {
    let refused = opened(Protocol::Http, "/users", FakeStreamTransport::contract()).await.unwrap_err();
    assert!(refused.contains("`sse` or `websocket`"), "{refused}");
}
