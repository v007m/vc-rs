use http::header::CONTENT_TYPE;
use http::{HeaderValue, Method, StatusCode, Uri};
use vc_interfaces::{
    MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport, TransportError,
};
use vc_model::{RenderedRequest, Size};

/// The contract every [`StreamTransport`] meets (D112).
///
/// `base` is a URL without a trailing slash whose server speaks the stream contract of
/// `vc-mock-server`: `GET /events?count={n}` answers 200 `text/event-stream` with the events
/// `id: {i}\ndata: {i}\n\n` for `i` from 1 to `n`, starting after a `last-event-id` header's
/// number when there is one, then ends; `GET /ws` is a WebSocket that echoes text and binary
/// frames and answers the text `bye` with a close 1000 `bye`; `GET /status/{code}` answers with
/// that status and the JSON body `{"status":{code}}`. [`crate::FakeStreamTransport::contract`] speaks it without a network.
///
/// # Panics
///
/// When the transport breaks the contract.
pub async fn stream_transport_suite(transport: &dyn StreamTransport, base: &str) {
    body_streams(transport, base).await;
    websockets(transport, base).await;
}

async fn body_streams(transport: &dyn StreamTransport, base: &str) {
    reads_an_event_stream_to_its_end(transport, base).await;
    resumes_after_the_last_event_id(transport, base).await;
    opens_no_stream_on_an_answer_that_is_not_a_success(transport, base).await;
    caps_a_refused_answer_s_body_at_the_limit(transport, base).await;
    refuses_to_send_on_a_body_stream(transport, base).await;
}

async fn websockets(transport: &dyn StreamTransport, base: &str) {
    opens_no_websocket_on_a_200(transport, base).await;
    hands_the_server_s_close_over_as_a_last_frame(transport, base).await;
    if let Some(stream) =
        open(transport, request(base, "/ws"), StreamMode::WebSocket).await.and_then(|opened| opened.stream)
    {
        message_stream_suite(stream).await;
    }
}

/// The contract every open WebSocket [`MessageStream`] to an echo server meets: it sends and
/// receives text and binary frames in order, and ends after the client closes.
///
/// # Panics
///
/// When the stream breaks the contract.
pub async fn message_stream_suite(mut stream: Box<dyn MessageStream>) {
    let frames = [OutgoingFrame::Text("hi".to_owned()), OutgoingFrame::Binary(bytes::Bytes::from_static(&[0, 255]))];
    for frame in frames {
        assert_eq!(stream.send(frame.clone()).await, Ok(()), "sending {frame:?}");
        assert_eq!(stream.next().await, Some(Ok(frame.into())), "the echo comes back as sent");
    }
    assert_eq!(stream.close(1000).await, Ok(()));
    assert_eq!(stream.next().await, None, "a closed stream ends");
}

/// Opens `request`; asserts the open succeeds, so `None` never reaches a caller.
async fn open(transport: &dyn StreamTransport, request: RenderedRequest, mode: StreamMode) -> Option<Opened> {
    let target = request.uri.clone();
    let result = transport.open(request, mode, &SendContext::default()).await;
    assert_eq!(result.as_ref().err(), None, "opening {target} as {mode:?} failed");
    result.ok()
}

fn request(base: &str, path: &str) -> RenderedRequest {
    RenderedRequest::new(Method::GET, Uri::try_from(format!("{base}{path}")).unwrap_or_default())
}

/// The stream `opened` holds; asserts there is one, so `None` never reaches a caller.
fn streamed(opened: Opened) -> Option<Box<dyn MessageStream>> {
    let status = opened.head.response.status;
    assert!(opened.stream.is_some(), "a {status} answer opens a stream");
    opened.stream
}

/// Every chunk the stream yields until it ends, as text.
async fn read_to_end(stream: &mut dyn MessageStream) -> String {
    let mut text = String::new();
    while let Some(frame) = stream.next().await {
        assert!(matches!(frame, Ok(StreamFrame::Chunk(_))), "a body stream yields chunks, not {frame:?}");
        if let Ok(StreamFrame::Chunk(chunk)) = frame {
            text.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
    text
}

async fn reads_an_event_stream_to_its_end(transport: &dyn StreamTransport, base: &str) {
    let Some(opened) = open(transport, request(base, "/events?count=2"), StreamMode::Body).await else { return };
    assert_eq!(opened.head.response.status, StatusCode::OK);
    let kind = opened.head.response.headers.get(CONTENT_TYPE).map(HeaderValue::as_bytes);
    assert!(kind.is_some_and(|kind| kind.starts_with(b"text/event-stream")), "{kind:?}");
    let Some(mut stream) = streamed(opened) else { return };
    assert_eq!(read_to_end(stream.as_mut()).await, "id: 1\ndata: 1\n\nid: 2\ndata: 2\n\n");
}

async fn resumes_after_the_last_event_id(transport: &dyn StreamTransport, base: &str) {
    let mut resumed = request(base, "/events?count=3");
    resumed.headers.insert("last-event-id", HeaderValue::from_static("1"));
    let Some(opened) = open(transport, resumed, StreamMode::Body).await else { return };
    let Some(mut stream) = streamed(opened) else { return };
    assert_eq!(read_to_end(stream.as_mut()).await, "id: 2\ndata: 2\n\nid: 3\ndata: 3\n\n");
}

async fn opens_no_stream_on_an_answer_that_is_not_a_success(transport: &dyn StreamTransport, base: &str) {
    for (mode, code) in [(StreamMode::Body, 503), (StreamMode::WebSocket, 404)] {
        let Some(opened) = open(transport, request(base, &format!("/status/{code}")), mode).await else { return };
        assert_eq!(opened.head.response.status.as_u16(), code, "{mode:?}");
        assert!(opened.stream.is_none(), "{mode:?}: a {code} answer opens no stream");
    }
    let Some(opened) = open(transport, request(base, "/ws"), StreamMode::WebSocket).await else { return };
    assert_eq!(opened.head.response.status, StatusCode::SWITCHING_PROTOCOLS);
}

async fn opens_no_websocket_on_a_200(transport: &dyn StreamTransport, base: &str) {
    let Some(opened) = open(transport, request(base, "/status/200"), StreamMode::WebSocket).await else { return };
    assert_eq!(opened.head.response.status, StatusCode::OK);
    assert!(opened.stream.is_none(), "a WebSocket opens only on its 101 handshake");
}

async fn caps_a_refused_answer_s_body_at_the_limit(transport: &dyn StreamTransport, base: &str) {
    let limit = Size::from_bytes(8);
    let ctx = SendContext { body_limit: limit, ..SendContext::default() };
    let refused = transport.open(request(base, "/status/503"), StreamMode::Body, &ctx).await;
    assert_eq!(refused.err(), Some(TransportError::BodyTooLarge { limit }), "a refused body over the limit");
    let Some(opened) = open(transport, request(base, "/status/503"), StreamMode::Body).await else { return };
    assert_eq!(opened.head.response.body.as_ref(), br#"{"status":503}"#, "a refused body is read in full");
}

async fn refuses_to_send_on_a_body_stream(transport: &dyn StreamTransport, base: &str) {
    let Some(opened) = open(transport, request(base, "/events?count=1"), StreamMode::Body).await else { return };
    let Some(mut stream) = streamed(opened) else { return };
    let sent = stream.send(OutgoingFrame::Text("hi".to_owned())).await;
    assert!(matches!(sent, Err(TransportError::InvalidRequest { .. })), "{sent:?}");
}

async fn hands_the_server_s_close_over_as_a_last_frame(transport: &dyn StreamTransport, base: &str) {
    let Some(opened) = open(transport, request(base, "/ws"), StreamMode::WebSocket).await else { return };
    let Some(mut stream) = streamed(opened) else { return };
    assert_eq!(stream.send(OutgoingFrame::Text("bye".to_owned())).await, Ok(()));
    assert_eq!(stream.next().await, Some(Ok(StreamFrame::Close { code: 1000, reason: "bye".to_owned() })));
    assert_eq!(stream.next().await, None, "nothing follows the server's close");
}
