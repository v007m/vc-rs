use http::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};
use http::{HeaderMap, HeaderValue, Method, Uri};
use tokio_tungstenite::tungstenite::error::CapacityError;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use vc_interfaces::{StreamFrame, TransportError};
use vc_model::{RenderedRequest, Size};

use super::{WebSocket, accepts, asks_to_upgrade, failed, over_http};

/// The key and accept of RFC 6455's example handshake (1.3).
const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";
const ACCEPT: &str = "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=";

fn sent_to(url: &str) -> Result<Uri, TransportError> {
    let uri = Uri::try_from(url).map_err(|error| TransportError::InvalidRequest { reason: error.to_string() })?;
    Ok(over_http(&RenderedRequest::new(Method::GET, uri))?.uri)
}

#[test]
fn ws_urls_go_as_http_and_others_as_they_are() -> Result<(), TransportError> {
    assert_eq!(sent_to("ws://localhost:8080/chat?room=1")?, "http://localhost:8080/chat?room=1");
    assert_eq!(sent_to("wss://example.com/chat")?, "https://example.com/chat");
    assert_eq!(sent_to("https://example.com/chat")?, "https://example.com/chat");
    assert_eq!(sent_to("ftp://example.com/x")?, "ftp://example.com/x", "the wire request refuses it later");
    Ok(())
}

#[test]
fn the_upgrade_headers_replace_the_request_s_own() -> Result<(), TransportError> {
    let mut headers = HeaderMap::new();
    headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
    headers.insert("sec-websocket-protocol", HeaderValue::from_static("chat"));
    asks_to_upgrade(&mut headers, KEY)?;
    let value = |name| headers.get(name).map(HeaderValue::as_bytes);
    assert_eq!((value(CONNECTION), value(UPGRADE)), (Some(&b"upgrade"[..]), Some(&b"websocket"[..])));
    assert_eq!((value(SEC_WEBSOCKET_VERSION), value(SEC_WEBSOCKET_KEY)), (Some(&b"13"[..]), Some(KEY.as_bytes())));
    assert_eq!(value(http::header::SEC_WEBSOCKET_PROTOCOL), Some(&b"chat"[..]), "a subprotocol is kept");
    Ok(())
}

fn answer(upgrade: &'static str, accept: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(UPGRADE, HeaderValue::from_static(upgrade));
    headers.insert(SEC_WEBSOCKET_ACCEPT, HeaderValue::from_static(accept));
    headers
}

#[test]
fn a_101_must_upgrade_to_websocket_and_accept_the_key() {
    assert_eq!(accepts(&answer("WebSocket", ACCEPT), KEY), Ok(()));
    assert!(matches!(accepts(&answer("websocket", "nope"), KEY), Err(TransportError::Protocol { .. })));
    assert!(matches!(accepts(&answer("h2c", ACCEPT), KEY), Err(TransportError::Protocol { .. })));
    assert!(matches!(accepts(&HeaderMap::new(), KEY), Err(TransportError::Protocol { .. })));
}

#[test]
fn a_frame_over_the_cap_is_too_large_and_other_errors_are_protocol_errors() {
    let limit = Size::from_bytes(8);
    let capacity = WsError::Capacity(CapacityError::MessageTooLong { size: 9, max_size: 8 });
    assert_eq!(failed(capacity, limit), TransportError::BodyTooLarge { limit });
    assert!(matches!(failed(WsError::AlreadyClosed, limit), TransportError::Protocol { .. }));
}

#[test]
fn messages_become_frames_and_a_close_ends_sending() {
    let mut socket = WebSocket { socket: None, open: true, limit: Size::from_bytes(8) };
    assert_eq!(socket.frame(Message::text("hi")), Some(StreamFrame::Text("hi".to_owned())));
    assert_eq!(socket.frame(Message::binary(vec![0, 255])), Some(StreamFrame::Binary(vec![0, 255].into())));
    assert_eq!(socket.frame(Message::Ping(Vec::new().into())), None, "tungstenite answers pings");
    assert!(socket.open);
    let close = CloseFrame { code: CloseCode::Away, reason: "later".into() };
    let closed = StreamFrame::Close { code: 1001, reason: "later".to_owned() };
    assert_eq!(socket.frame(Message::Close(Some(close))), Some(closed));
    assert!(!socket.open, "nothing is sent after the server's close");
    let bare = StreamFrame::Close { code: 1005, reason: String::new() };
    assert_eq!(socket.frame(Message::Close(None)), Some(bare), "a close with no code is 1005");
}
