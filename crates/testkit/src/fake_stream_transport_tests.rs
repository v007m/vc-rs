use std::time::Duration;

use http::{Method, StatusCode, Uri};
use vc_interfaces::{SendContext, StreamFrame, StreamMode, StreamTransport, TransportError};
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
    assert_eq!(stream.send(StreamFrame::Text("dropped".into())).await, Ok(()));
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Text("a".into()))));
    assert_eq!(stream.next().await, Some(Err(TransportError::Protocol { reason: "reset".into() })));
    assert_eq!(stream.next().await, None, "a scripted stream does not echo");
    assert!(fake.open(get("/"), StreamMode::WebSocket, &ctx).await.unwrap().stream.is_some());
    assert_eq!(clone.calls(), 3, "clones share the count");
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
