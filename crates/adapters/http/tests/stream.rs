//! `HttpTransport` as the stream port against `vc-mock-server`: the stream contract, the head
//! it hands over, a stream that stays open, what it refuses (D427), and a WebSocket over `ws`,
//! over TLS and past the body limit (D460–D462).

use std::error::Error;
use std::time::Duration;

use http::header::{ACCEPT_ENCODING, CONTENT_TYPE, UPGRADE};
use http::{HeaderValue, Method, StatusCode, Uri};
use serde_json::Value;
use vc_adapter_http::{HttpOptions, HttpTransport, TlsOptions};
use vc_interfaces::{
    MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport, Transport,
    TransportError,
};
use vc_mock_server::{MockHandle, MockServer, MockTls};
use vc_model::{HttpVersion, RenderedRequest, Size};
use vc_testkit::TestPki;

type TestResult = Result<(), Box<dyn Error>>;

async fn open(mock: &MockHandle, path: &str, mode: StreamMode) -> Result<Opened, TransportError> {
    let uri = Uri::try_from(format!("{}{path}", mock.base_url())).unwrap_or_default();
    let transport = HttpTransport::new(&HttpOptions::default())
        .map_err(|error| TransportError::Protocol { reason: error.to_string() })?;
    transport.open(RenderedRequest::new(Method::GET, uri), mode, &SendContext::default()).await
}

async fn open_at(url: &str, ctx: &SendContext) -> Result<Opened, Box<dyn Error>> {
    let transport = HttpTransport::new(&HttpOptions::default())?;
    Ok(transport.open(RenderedRequest::new(Method::GET, Uri::try_from(url)?), StreamMode::WebSocket, ctx).await?)
}

async fn text_of(stream: &mut dyn MessageStream) -> String {
    let mut text = String::new();
    while let Some(Ok(StreamFrame::Chunk(chunk))) = stream.next().await {
        text.push_str(&String::from_utf8_lossy(&chunk));
    }
    text
}

#[tokio::test]
async fn meets_the_stream_transport_contract() -> TestResult {
    let mock = MockServer::start().await?;
    vc_testkit::stream_transport_suite(&HttpTransport::new(&HttpOptions::default())?, &mock.base_url()).await;
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
async fn a_ws_url_opens_a_websocket_and_the_head_keeps_it() -> TestResult {
    let mock = MockServer::start().await?;
    let url = mock.base_url().replacen("http://", "ws://", 1);
    let opened = open_at(&format!("{url}/ws"), &SendContext::default()).await?;
    assert_eq!(opened.head.response.status, StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(opened.head.request.uri.scheme_str(), Some("ws"), "the head keeps the URL as written");
    vc_testkit::message_stream_suite(opened.stream.ok_or("no stream")?).await;
    Ok(())
}

/// The https mock offering h2, and a transport that trusts it.
async fn h2_mock(pki: &TestPki) -> Result<(MockHandle, HttpTransport), Box<dyn Error>> {
    let server = pki.server();
    let tls = MockTls::new(vec![server.cert_der().to_vec()], server.key_der().to_vec()).h2();
    let tls_options = TlsOptions { ca: vec![pki.ca_pem().into_bytes()], ..TlsOptions::default() };
    Ok((
        MockServer::start_tls(tls).await?,
        HttpTransport::new(&HttpOptions { tls: tls_options, ..HttpOptions::default() })?,
    ))
}

#[tokio::test]
async fn a_wss_websocket_opens_over_http1_on_a_server_that_offers_h2() -> TestResult {
    let pki = TestPki::new()?;
    let (mock, transport) = h2_mock(&pki).await?;
    let url = format!("{}/ws", mock.base_url().replacen("https://", "wss://", 1));
    let request = RenderedRequest::new(Method::GET, Uri::try_from(url)?);
    let opened = transport.open(request, StreamMode::WebSocket, &SendContext::default()).await?;
    let head = (opened.head.response.status, opened.head.connection.protocol);
    assert_eq!(head, (StatusCode::SWITCHING_PROTOCOLS, HttpVersion::Http1));
    vc_testkit::message_stream_suite(opened.stream.ok_or("no stream")?).await;
    let mut echo = RenderedRequest::new(Method::GET, Uri::try_from(format!("{}/echo", mock.base_url()))?);
    echo.headers.insert(UPGRADE, HeaderValue::from_static("h2c"));
    let sent = transport.send(echo, &SendContext::default()).await?;
    assert_eq!(sent.connection.protocol, HttpVersion::Http2, "other requests, even with an `upgrade`, still speak h2");
    Ok(())
}

#[tokio::test]
async fn an_h2_stream_hands_over_no_empty_chunk() -> TestResult {
    let pki = TestPki::new()?;
    let (mock, transport) = h2_mock(&pki).await?;
    let uri = Uri::try_from(format!("{}/events?count=1", mock.base_url()))?;
    let opened =
        transport.open(RenderedRequest::new(Method::GET, uri), StreamMode::Body, &SendContext::default()).await?;
    assert_eq!(opened.head.connection.protocol, HttpVersion::Http2);
    let mut stream = opened.stream.ok_or("no stream")?;
    let mut frames = Vec::new();
    while let Some(frame) = stream.next().await {
        frames.push(frame);
    }
    let event = StreamFrame::Chunk(bytes::Bytes::from_static(b"id: 1\ndata: 1\n\n"));
    assert_eq!(frames, [Ok(event)], "h2 ends a body with an empty DATA frame, which is no chunk");
    Ok(())
}

#[tokio::test]
async fn a_message_over_the_body_limit_is_too_large() -> TestResult {
    let mock = MockServer::start().await?;
    let limit = Size::from_bytes(8);
    let ctx = SendContext { body_limit: limit, ..SendContext::default() };
    let mut stream = open_at(&format!("{}/ws", mock.base_url()), &ctx).await?.stream.ok_or("no stream")?;
    assert_eq!(stream.send(OutgoingFrame::Text("nine bytes".to_owned())).await, Ok(()), "only what is read is capped");
    assert_eq!(stream.next().await, Some(Err(TransportError::BodyTooLarge { limit })));
    assert_eq!(stream.next().await, None, "an error ends the stream");
    Ok(())
}
