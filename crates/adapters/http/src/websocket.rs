//! A WebSocket over HTTP (D460–D464): a `GET` asking to upgrade, sent through the pooled client
//! that offers only HTTP/1.1, so it shares the connector's timings, proxy, roots and client
//! certificates. A 101 answer that accepts the key upgrades the connection, and tungstenite
//! reads and writes its frames.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use http::header::{
    CONNECTION, HeaderName, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_EXTENSIONS, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION,
    UPGRADE,
};
use http::uri::Scheme;
use http::{HeaderMap, HeaderValue, Uri};
use hyper::upgrade::{OnUpgrade, Upgraded};
use hyper_util::rt::TokioIo;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::{client::generate_key, derive_accept_key};
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Role, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use vc_interfaces::{MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, TransportError};
use vc_model::{RenderedRequest, Size};

use crate::HttpTransport;
use crate::transport::Upgrading;

/// How long [`MessageStream::close`] waits for the server to answer its close (D463).
const CLOSE_WAIT: Duration = Duration::from_secs(1);

/// The close code a close frame with no code stands for (RFC 6455, 7.1.5).
const NO_STATUS: u16 = 1005;

type Socket = WebSocketStream<TokioIo<Upgraded>>;

/// Opens `request` as a WebSocket: `ws` and `wss` URLs go as `http` and `https`, and the head
/// keeps the URL as written. An answer other than 101 opens no stream, its body read in full
/// under `ctx.body_limit`; a 101 that does not accept the key is a protocol error.
pub async fn open(
    transport: &HttpTransport,
    request: RenderedRequest,
    ctx: &SendContext,
) -> Result<Opened, TransportError> {
    let key = generate_key();
    let mut wire = transport.wire_request(&over_http(&request)?)?;
    asks_to_upgrade(wire.headers_mut(), &key)?;
    wire.extensions_mut().insert(Upgrading);
    let mut answered = transport.answer(wire, ctx).await?;
    if !StreamMode::WebSocket.opens_on().matches(answered.parts.status.as_u16()) {
        return Ok(Opened { head: answered.read(request, ctx.body_limit).await?, stream: None });
    }
    accepts(&answered.parts.headers, &key)?;
    let upgrade = answered.parts.extensions.remove::<OnUpgrade>();
    let (head, _empty) = answered.streamed(request);
    let upgraded = upgrade.ok_or_else(|| protocol("the 101 answer left no connection to upgrade"))?;
    let upgraded = upgraded.await.map_err(|error| protocol(&error.to_string()))?;
    let config = config(ctx.body_limit);
    let socket = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Client, Some(config)).await;
    Ok(Opened { head, stream: Some(Box::new(WebSocket { socket: Some(socket), open: true, limit: ctx.body_limit })) })
}

/// `request` with a `ws` URL as `http` and a `wss` one as `https`; any other as it is.
fn over_http(request: &RenderedRequest) -> Result<RenderedRequest, TransportError> {
    let scheme = match request.uri.scheme_str() {
        Some("ws") => Scheme::HTTP,
        Some("wss") => Scheme::HTTPS,
        _ => return Ok(request.clone()),
    };
    let mut parts = request.uri.clone().into_parts();
    parts.scheme = Some(scheme);
    let uri = Uri::from_parts(parts).map_err(|error| TransportError::InvalidRequest { reason: error.to_string() })?;
    Ok(RenderedRequest { uri, ..request.clone() })
}

/// Sets the upgrade headers, over any the request names itself, and drops its
/// `sec-websocket-extensions`: tungstenite reads no compressed frames.
fn asks_to_upgrade(headers: &mut HeaderMap, key: &str) -> Result<(), TransportError> {
    let key =
        HeaderValue::try_from(key).map_err(|error| TransportError::InvalidRequest { reason: error.to_string() })?;
    headers.remove(SEC_WEBSOCKET_EXTENSIONS);
    headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"));
    headers.insert(SEC_WEBSOCKET_KEY, key);
    Ok(())
}

/// `Ok` when a 101's headers upgrade the connection to `websocket`, accept `key` and agree no
/// extension, as none was asked for (RFC 6455, 4.1).
fn accepts(headers: &HeaderMap, key: &str) -> Result<(), TransportError> {
    let accept = derive_accept_key(key.as_bytes());
    let accepted = headers.get(SEC_WEBSOCKET_ACCEPT).is_some_and(|value| value.as_bytes() == accept.as_bytes());
    let upgraded = names(headers, &CONNECTION, "upgrade") && names(headers, &UPGRADE, "websocket");
    if accepted && upgraded && !headers.contains_key(SEC_WEBSOCKET_EXTENSIONS) {
        return Ok(());
    }
    Err(protocol(
        "the 101 answer does not accept the WebSocket: its `connection`, `upgrade` or `sec-websocket-accept` is wrong, or it names an extension",
    ))
}

/// Whether the `name` header lists `token`, in any case.
fn names(headers: &HeaderMap, name: &HeaderName, token: &str) -> bool {
    let listed = |value: &HeaderValue| {
        value.to_str().is_ok_and(|text| text.split(',').any(|part| part.trim().eq_ignore_ascii_case(token)))
    };
    headers.get_all(name).iter().any(listed)
}

/// Messages and frames capped at `limit`, as a body is (D462).
fn config(limit: Size) -> WebSocketConfig {
    let max = usize::try_from(limit.bytes()).ok();
    WebSocketConfig::default().max_message_size(max).max_frame_size(max)
}

fn protocol(reason: &str) -> TransportError {
    TransportError::Protocol { reason: reason.to_owned() }
}

/// A frame read or write that failed: one over the cap is [`TransportError::BodyTooLarge`].
fn failed(error: WsError, limit: Size) -> TransportError {
    match error {
        WsError::Capacity(_) => TransportError::BodyTooLarge { limit },
        other => protocol(&other.to_string()),
    }
}

/// An open WebSocket. `open` turns false once either side closes, after which it sends nothing;
/// it ends when the connection does, at its first error, or at [`MessageStream::close`].
struct WebSocket {
    socket: Option<Socket>,
    open: bool,
    limit: Size,
}

impl WebSocket {
    /// The frame `message` hands over: pings and pongs are answered by tungstenite and skipped.
    fn frame(&mut self, message: Message) -> Option<StreamFrame> {
        match message {
            Message::Text(text) => Some(StreamFrame::Text(text.as_str().to_owned())),
            Message::Binary(bytes) => Some(StreamFrame::Binary(bytes)),
            Message::Close(frame) => {
                self.open = false;
                let (code, reason) = frame.map_or((NO_STATUS, String::new()), |frame| {
                    (u16::from(frame.code), frame.reason.as_str().to_owned())
                });
                Some(StreamFrame::Close { code, reason })
            }
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => None,
        }
    }
}

#[async_trait]
impl MessageStream for WebSocket {
    async fn send(&mut self, frame: OutgoingFrame) -> Result<(), TransportError> {
        let socket = self.socket.as_mut().filter(|_| self.open);
        let Some(socket) = socket else {
            return Err(TransportError::InvalidRequest { reason: "the WebSocket is closed".to_owned() });
        };
        let message = match frame {
            OutgoingFrame::Text(text) => Message::text(text),
            OutgoingFrame::Binary(bytes) => Message::Binary(bytes),
        };
        socket.send(message).await.map_err(|error| failed(error, self.limit))
    }

    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>> {
        loop {
            let read = self.socket.as_mut()?.next().await;
            match read {
                Some(Ok(message)) => {
                    if let Some(frame) = self.frame(message) {
                        return Some(Ok(frame));
                    }
                }
                Some(Err(error)) => {
                    self.socket = None;
                    return Some(Err(failed(error, self.limit)));
                }
                None => {
                    self.socket = None;
                    return None;
                }
            }
        }
    }

    /// Sends a close with `code` (or, after the server's close, its answer), waits for the server
    /// to end the connection, and drops it, all within [`CLOSE_WAIT`]. Closing is cleanup, so a
    /// connection that already failed, or a server that stops reading, is not an error.
    async fn close(&mut self, code: u16) -> Result<(), TransportError> {
        self.open = false;
        let Some(mut socket) = self.socket.take() else { return Ok(()) };
        let frame = CloseFrame { code: code.into(), reason: "".into() };
        let closed = async {
            if socket.close(Some(frame)).await.is_ok() {
                while let Some(Ok(_)) = socket.next().await {}
            }
        };
        let _ = tokio::time::timeout(CLOSE_WAIT, closed).await;
        Ok(())
    }
}

#[cfg(test)]
#[path = "websocket_tests.rs"]
mod tests;
