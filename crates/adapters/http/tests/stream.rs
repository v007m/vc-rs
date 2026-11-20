//! `HttpTransport` as the stream port against `vc-mock-server`: the body-stream contract, the
//! head it hands over, a stream that stays open, and what it refuses (D427).

use std::error::Error;
use std::time::Duration;

use http::header::{ACCEPT_ENCODING, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode, Uri};
use serde_json::Value;
use vc_adapter_http::{HttpOptions, HttpTransport};
use vc_interfaces::{MessageStream, Opened, SendContext, StreamFrame, StreamMode, StreamTransport, TransportError};
use vc_mock_server::{MockHandle, MockServer};
use vc_model::RenderedRequest;

type TestResult = Result<(), Box<dyn Error>>;

async fn open(mock: &MockHandle, path: &str, mode: StreamMode) -> Result<Opened, TransportError> {
    let uri = Uri::try_from(format!("{}{path}", mock.base_url())).unwrap_or_default();
    let transport = HttpTransport::new(&HttpOptions::default())
        .map_err(|error| TransportError::Protocol { reason: error.to_string() })?;
    transport.open(RenderedRequest::new(Method::GET, uri), mode, &SendContext::default()).await
}

async fn text_of(stream: &mut dyn MessageStream) -> String {
    let mut text = String::new();
    while let Some(Ok(StreamFrame::Chunk(chunk))) = stream.next().await {
        text.push_str(&String::from_utf8_lossy(&chunk));
    }
    text
}

#[tokio::test]
async fn meets_the_body_stream_contract() -> TestResult {
    let mock = MockServer::start().await?;
    vc_testkit::body_stream_suite(&HttpTransport::new(&HttpOptions::default())?, &mock.base_url()).await;
    Ok(())
}

#[tokio::test]
async fn the_head_is_the_answer_without_its_body() -> TestResult {
    let mock = MockServer::start().await?;
    let opened = open(&mock, "/events?count=1", StreamMode::Body).await?;
    let head = &opened.head;
    assert_eq!((head.response.status, head.response.body.len(), head.response.wire_size), (StatusCode::OK, 0, 0));
    assert_eq!(head.response.headers.get(CONTENT_TYPE), Some(&HeaderValue::from_static("text/event-stream")));
    assert_eq!(head.timing.total, head.timing.first_byte, "the head's time ends at its first byte");
    assert!(head.timing.connect.is_some() && !head.connection.reused, "a first request opens a connection");
    assert_eq!(head.request.uri.path(), "/events");
    Ok(())
}

#[tokio::test]
async fn a_stream_request_is_sent_as_rendered_with_no_accept_encoding() -> TestResult {
    let mock = MockServer::start().await?;
    let Opened { head, stream } = open(&mock, "/echo", StreamMode::Body).await?;
    assert_eq!(head.response.status, StatusCode::OK, "any 2xx opens a body stream");
    let echoed: Value = serde_json::from_str(&text_of(stream.ok_or("no stream")?.as_mut()).await)?;
    assert_eq!(echoed["headers"]["accept-encoding"], Value::Null, "{echoed}");
    Ok(())
}

#[tokio::test]
async fn an_encoded_stream_is_refused() -> TestResult {
    let mock = MockServer::start().await?;
    let uri = Uri::try_from(format!("{}/large?kb=50", mock.base_url()))?;
    let mut request = RenderedRequest::new(Method::GET, uri);
    request.headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
    let transport = HttpTransport::new(&HttpOptions::default())?;
    let refused = transport.open(request, StreamMode::Body, &SendContext::default()).await;
    assert!(matches!(&refused, Err(TransportError::Decode { encoding, .. }) if encoding == "gzip"), "{refused:?}");
    Ok(())
}

#[tokio::test]
async fn an_open_stream_waits_and_ends_when_closed() -> TestResult {
    let mock = MockServer::start().await?;
    let opened = open(&mock, "/events?count=1&hold=true", StreamMode::Body).await?;
    let mut stream = opened.stream.ok_or("no stream")?;
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Chunk(bytes::Bytes::from_static(b"id: 1\ndata: 1\n\n")))));
    let waited = tokio::time::timeout(Duration::from_millis(150), stream.next()).await;
    assert!(waited.is_err(), "a silent open stream waits: {waited:?}");
    assert_eq!(stream.close(1000).await, Ok(()));
    assert_eq!(stream.next().await, None, "a closed stream ends");
    Ok(())
}

#[tokio::test]
async fn a_websocket_waits_for_its_adapter() -> TestResult {
    let mock = MockServer::start().await?;
    let refused = open(&mock, "/events", StreamMode::WebSocket).await;
    assert!(matches!(refused, Err(TransportError::InvalidRequest { .. })), "{refused:?}");
    assert_eq!(mock.stats().await?.count("GET /events"), 0, "nothing is sent");
    Ok(())
}
