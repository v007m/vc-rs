use async_trait::async_trait;
use bytes::Bytes;
use vc_model::{Exchange, RenderedRequest};

use crate::{SendContext, TransportError};

/// Opens one streaming exchange: an SSE response body or a WebSocket (D382).
///
/// Like [`crate::Transport`] it knows nothing of auth, retries or per-host limits; the
/// executor's request pipeline wraps the open. The HTTP adapter implements both ports;
/// `vc-testkit` has a fake, and every implementation runs `stream_transport_suite` (D112).
#[async_trait]
pub trait StreamTransport: Send + Sync {
    /// Sends `request` and opens a stream on a 2xx answer (`Body`) or a 101 handshake
    /// (`WebSocket`). Any other answer is a successful open with no stream, its body read in full
    /// and capped at `ctx.body_limit` as [`crate::Transport::send`] reads it.
    ///
    /// # Errors
    ///
    /// A [`TransportError`] when no complete response head arrived.
    async fn open(
        &self,
        request: RenderedRequest,
        mode: StreamMode,
        ctx: &SendContext,
    ) -> Result<Opened, TransportError>;
}

/// How the stream is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub enum StreamMode {
    /// The response body, as byte chunks (`sse`).
    Body,
    /// WebSocket frames after an HTTP/1.1 upgrade (`websocket`).
    WebSocket,
}

/// An opened stream: the opening response, and the stream when one opened.
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Opened {
    /// The opening response: the SSE response head, or the WebSocket handshake. Its body is
    /// empty when a stream opened.
    pub head: Exchange,
    /// The stream, or `None` when the answer opened none.
    pub stream: Option<Box<dyn MessageStream>>,
}

impl std::fmt::Debug for Opened {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opened").field("head", &self.head).field("stream", &self.stream.is_some()).finish()
    }
}

/// One open stream. The adapter answers pings itself; pings and pongs are never surfaced.
#[async_trait]
pub trait MessageStream: Send {
    /// Sends `frame` on a WebSocket.
    ///
    /// # Errors
    ///
    /// [`TransportError::InvalidRequest`] on a body stream, which cannot send, or a
    /// [`TransportError`] when the connection broke.
    async fn send(&mut self, frame: StreamFrame) -> Result<(), TransportError>;

    /// The next frame, or `None` once the stream ended. A server's WebSocket close is one final
    /// [`StreamFrame::Close`].
    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>>;

    /// Ends the stream: a WebSocket sends a close with `code` and waits for the server's; a body
    /// stream is dropped. Later calls to `next` return `None`.
    ///
    /// # Errors
    ///
    /// A [`TransportError`] when the close could not be sent or answered.
    async fn close(&mut self, code: u16) -> Result<(), TransportError>;
}

/// One unit read from or sent on a stream.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub enum StreamFrame {
    /// Bytes of a response body, split wherever the network split them.
    Chunk(Bytes),
    /// A WebSocket text frame.
    Text(String),
    /// A WebSocket binary frame.
    Binary(Bytes),
    /// A WebSocket close.
    Close {
        /// The close code.
        code: u16,
        /// The close reason.
        reason: String,
    },
}
