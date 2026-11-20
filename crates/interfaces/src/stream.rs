use async_trait::async_trait;
use bytes::Bytes;
use vc_model::{Exchange, Protocol, RenderedRequest, StatusPattern};

use crate::{SendContext, TransportError};

/// Opens one streaming exchange: an SSE response body or a WebSocket (D382).
///
/// Like [`crate::Transport`] it knows nothing of auth, retries or per-host limits; the
/// executor's request pipeline wraps the open. The HTTP adapter implements both ports;
/// `vc-testkit` has a fake, and every implementation runs `stream_transport_suite` (D112).
#[async_trait]
pub trait StreamTransport: Send + Sync {
    /// Sends `request` and opens a stream only on an answer whose status is the `mode`'s
    /// [`StreamMode::opens_on`]. Any other answer is refused: a successful open with no stream,
    /// its body read in full and capped at `ctx.body_limit` as [`crate::Transport::send`] reads it.
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

impl StreamMode {
    /// The status a stream of this mode opens on: the success status of the type it reads.
    #[must_use]
    pub const fn opens_on(self) -> StatusPattern {
        let protocol = match self {
            Self::Body => Protocol::Sse,
            Self::WebSocket => Protocol::Websocket,
        };
        protocol.success_status()
    }
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
    /// Sends `frame` on a WebSocket. A close is sent only by [`MessageStream::close`].
    ///
    /// # Errors
    ///
    /// [`TransportError::InvalidRequest`] on a body stream, which cannot send, a
    /// [`TransportError`] after [`MessageStream::close`], or one when the connection broke.
    async fn send(&mut self, frame: OutgoingFrame) -> Result<(), TransportError>;

    /// The next frame, waiting for one while the stream is open, or `None` once it ended; never
    /// `None` before then. A server's WebSocket close is one final [`StreamFrame::Close`]; an
    /// `Err` ends the stream too. Nothing follows a `Close` or an `Err`: every later call returns
    /// `None`.
    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>>;

    /// Ends the stream: a WebSocket sends a close with `code` and waits for the server's; a body
    /// stream is dropped. Later calls to `next` return `None`, and to `send` an error.
    ///
    /// # Errors
    ///
    /// A [`TransportError`] when the close could not be sent or answered.
    async fn close(&mut self, code: u16) -> Result<(), TransportError>;
}

/// One frame a client sends on a WebSocket: data only, as [`MessageStream::close`] sends the
/// close.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub enum OutgoingFrame {
    /// A text frame.
    Text(String),
    /// A binary frame.
    Binary(Bytes),
}

impl From<OutgoingFrame> for StreamFrame {
    fn from(frame: OutgoingFrame) -> Self {
        match frame {
            OutgoingFrame::Text(text) => Self::Text(text),
            OutgoingFrame::Binary(bytes) => Self::Binary(bytes),
        }
    }
}

/// One unit read from a stream.
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

#[cfg(test)]
mod tests {
    use vc_model::StatusPattern;

    use super::{OutgoingFrame, StreamFrame, StreamMode};

    #[test]
    fn a_body_stream_opens_on_2xx_and_a_websocket_on_its_101_handshake() {
        let opens_on = [StreamMode::Body, StreamMode::WebSocket].map(StreamMode::opens_on);
        assert_eq!(opens_on, [StatusPattern::Class(2), StatusPattern::Code(101)]);
    }

    #[test]
    fn an_outgoing_frame_reads_back_as_the_same_frame_received() {
        let text = StreamFrame::from(OutgoingFrame::Text("hi".to_owned()));
        let binary = StreamFrame::from(OutgoingFrame::Binary(bytes::Bytes::from_static(&[0, 255])));
        assert_eq!(
            (text, binary),
            (StreamFrame::Text("hi".to_owned()), StreamFrame::Binary(bytes::Bytes::from_static(&[0, 255])))
        );
    }
}
