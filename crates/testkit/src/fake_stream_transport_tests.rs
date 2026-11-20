use std::time::Duration;

use http::{Method, StatusCode, Uri};
use vc_interfaces::{OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport, TransportError};
use vc_model::{RenderedRequest, Size};

use super::{FakeStreamTransport, StreamAnswer};
use crate::{status, stream_transport_suite};

fn get(path: &str) -> RenderedRequest {
    RenderedRequest::new(Method::GET, Uri::try_from(format!("http://fake.test{path}")).unwrap())
}

#[tokio::test]
async fn the_contract_fake_passes_the_stream_transport_suite() {
    stream_transport_suite(&FakeStreamTransport::contract(), "http://fake.test").await;
}

#[tokio::test]
async fn the_contract_answers_404_off_its_paths_and_400_for_a_bad_status() {
    let fake = FakeStreamTransport::contract();
    let ctx = SendContext::default();
    for (path, mode, code) in [
        ("/ws", StreamMode::Body, StatusCode::NOT_FOUND),
        ("/events", StreamMode::WebSocket, StatusCode::NOT_FOUND),
        ("/status/abc", StreamMode::Body, StatusCode::BAD_REQUEST),
    ] {
        let opened = fake.open(get(path), mode, &ctx).await.unwrap();
        assert_eq!((opened.head.response.status, opened.stream.is_none()), (code, true), "{path} {mode:?}");
    }
    let mut empty = fake.open(get("/events"), StreamMode::Body, &ctx).await.unwrap().stream.unwrap();
    assert_eq!(empty.next().await, None, "no count, no events");
    assert_eq!(fake.calls(), 4);
}

#[tokio::test]
async fn a_script_plays_in_order_then_repeats_its_last_entry() {
    let timeout = TransportError::Timeout { after: Duration::from_millis(1) };
    let frames = vec![Ok(StreamFrame::Text("a".into())), Err(TransportError::Protocol { reason: "reset".into() })];
    let fake = FakeStreamTransport::scripted(vec![Err(timeout.clone()), Ok(StreamAnswer::stream(status(101), frames))]);
    let clone = fake.clone();
    let ctx = SendContext::default();
    assert_eq!(fake.open(get("/"), StreamMode::WebSocket, &ctx).await.err(), Some(timeout));
    let mut stream = clone.open(get("/"), StreamMode::WebSocket, &ctx).await.unwrap().stream.unwrap();
    assert_eq!(stream.send(OutgoingFrame::Text("dropped".into())).await, Ok(()));
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Text("a".into()))));
    assert_eq!(stream.next().await, Some(Err(TransportError::Protocol { reason: "reset".into() })));
    assert_eq!(stream.next().await, None, "a scripted stream does not echo");
    assert!(fake.open(get("/"), StreamMode::WebSocket, &ctx).await.unwrap().stream.is_some());
    assert_eq!(clone.calls(), 3, "clones share the count");
}

#[tokio::test]
async fn a_stream_opens_only_on_its_mode_s_success_head() {
    let ctx = SendContext::default();
    let text = || vec![Ok(StreamFrame::Text("a".into()))];
    for (answer, mode, opens) in [
        (StreamAnswer::stream(status(200), text()), StreamMode::Body, true),
        (StreamAnswer::stream(status(200), text()), StreamMode::WebSocket, false),
        (StreamAnswer::stream(status(101), text()), StreamMode::Body, false),
        (StreamAnswer::echo(), StreamMode::WebSocket, true),
        (StreamAnswer::echo(), StreamMode::Body, false),
    ] {
        let fake = FakeStreamTransport::scripted(vec![Ok(answer.clone())]);
        let opened = fake.open(get("/"), mode, &ctx).await.unwrap();
        assert_eq!(opened.stream.is_some(), opens, "{answer:?} as {mode:?}");
    }
}

/// After a close frame or an error, `send` is an error and queues nothing, even on an echo
/// stream (D388).
#[tokio::test]
async fn a_scripted_stream_ends_at_a_close_or_an_error() {
    let close = Ok(StreamFrame::Close { code: 1001, reason: "away".into() });
    let reset = Err(TransportError::Protocol { reason: "reset".into() });
    let after = Ok(StreamFrame::Text("after".into()));
    for last in [close, reset] {
        let fake = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::stream(
            status(101),
            vec![last.clone(), after.clone()],
        ))]);
        let mut stream =
            fake.open(get("/"), StreamMode::WebSocket, &SendContext::default()).await.unwrap().stream.unwrap();
        assert_eq!(stream.next().await, Some(last));
        let sent = stream.send(OutgoingFrame::Text("hi".into())).await;
        assert!(matches!(sent, Err(TransportError::InvalidRequest { .. })), "{sent:?}");
        assert_eq!((stream.next().await, stream.next().await), (None, None), "nothing follows");
    }
    let fake = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::echo())]);
    let mut echo = fake.open(get("/"), StreamMode::WebSocket, &SendContext::default()).await.unwrap().stream.unwrap();
    assert_eq!(echo.send(OutgoingFrame::Text("bye".into())).await, Ok(()));
    assert!(matches!(echo.next().await, Some(Ok(StreamFrame::Close { .. }))));
    assert!(echo.send(OutgoingFrame::Text("hi".into())).await.is_err(), "the server closed");
    assert_eq!(echo.next().await, None, "nothing was queued");
}

#[tokio::test]
async fn a_scripted_stream_that_runs_out_of_frames_has_ended_so_send_is_an_error() {
    let fake = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::stream(
        status(101),
        vec![Ok(StreamFrame::Text("a".into()))],
    ))]);
    let mut stream = fake.open(get("/"), StreamMode::WebSocket, &SendContext::default()).await.unwrap().stream.unwrap();
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Text("a".into()))));
    assert_eq!(stream.next().await, None, "the script ran out");
    let sent = stream.send(OutgoingFrame::Text("hi".into())).await;
    assert!(matches!(sent, Err(TransportError::InvalidRequest { .. })), "{sent:?}");
    assert_eq!(stream.next().await, None, "nothing was queued");
}

#[tokio::test]
async fn an_echo_stream_refuses_to_send_once_its_close_is_queued() {
    let fake = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::echo())]);
    let mut stream = fake.open(get("/"), StreamMode::WebSocket, &SendContext::default()).await.unwrap().stream.unwrap();
    assert_eq!(stream.send(OutgoingFrame::Text("bye".into())).await, Ok(()));
    let sent = stream.send(OutgoingFrame::Text("hi".into())).await;
    assert!(matches!(sent, Err(TransportError::InvalidRequest { .. })), "the close is queued: {sent:?}");
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Close { code: 1000, reason: "bye".into() })));
    assert_eq!(stream.next().await, None, "the refused send queued nothing");
}

#[tokio::test]
async fn an_echo_stream_with_nothing_queued_waits_as_a_silent_server_would() {
    let fake = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::echo())]);
    let mut stream = fake.open(get("/"), StreamMode::WebSocket, &SendContext::default()).await.unwrap().stream.unwrap();
    let waited = tokio::time::timeout(Duration::from_millis(20), stream.next()).await;
    assert!(waited.is_err(), "an open echo stream is not over: {waited:?}");
    assert_eq!(stream.send(OutgoingFrame::Text("hi".into())).await, Ok(()));
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Text("hi".into()))));
    assert_eq!(stream.close(1000).await, Ok(()));
    assert_eq!(stream.next().await, None, "a closed stream ends at once");
}

#[tokio::test]
async fn an_empty_script_answers_404_with_no_stream() {
    let fake = FakeStreamTransport::scripted(Vec::new());
    let opened = fake.open(get("/"), StreamMode::Body, &SendContext::default()).await.unwrap();
    assert_eq!((opened.head.response.status, opened.stream.is_none()), (StatusCode::NOT_FOUND, true));
}

#[tokio::test]
async fn a_refused_body_over_the_limit_is_too_large() {
    let mut head = status(401);
    head.body = bytes::Bytes::from_static(b"0123456789");
    let fake = FakeStreamTransport::scripted(vec![Ok(StreamAnswer::refused(head))]);
    let limit = Size::from_bytes(8);
    let ctx = SendContext { body_limit: limit, ..SendContext::default() };
    let opened = fake.open(get("/"), StreamMode::Body, &ctx).await;
    assert_eq!(opened.err(), Some(TransportError::BodyTooLarge { limit }));
    let roomy = SendContext { body_limit: Size::from_bytes(10), ..ctx };
    assert!(fake.open(get("/"), StreamMode::Body, &roomy).await.is_ok(), "a body at the limit is kept");
}
