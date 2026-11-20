//! The stream port over HTTP (D427): a body stream is the answer's body, handed over chunk by
//! chunk as it arrives. A WebSocket waits for PR 6 of the streams plan.

use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use vc_interfaces::{
    MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport, TransportError,
};
use vc_model::RenderedRequest;

use crate::{HttpTransport, body};

#[async_trait]
impl StreamTransport for HttpTransport {
    /// Opens `request` as a body stream. A 2xx answer streams its body as it comes; any other
    /// answer opens no stream, its body read in full under `ctx.body_limit`. The request is sent
    /// as rendered, with no `accept-encoding` added, and a streamed body that is encoded anyway
    /// is refused, since it cannot be decoded a chunk at a time.
    async fn open(
        &self,
        request: RenderedRequest,
        mode: StreamMode,
        ctx: &SendContext,
    ) -> Result<Opened, TransportError> {
        if mode != StreamMode::Body {
            return Err(TransportError::InvalidRequest {
                reason: "WebSocket streams are not supported yet".to_owned(),
            });
        }
        let answered = self.answer(self.wire_request(&request)?, ctx).await?;
        if !mode.opens_on().matches(answered.parts.status.as_u16()) {
            return Ok(Opened { head: answered.read(request, ctx.body_limit).await?, stream: None });
        }
        if let Some(encoding) = body::encodings(&answered.parts.headers).into_iter().next() {
            let reason = "a stream is read as it arrives, so its body cannot be encoded".to_owned();
            return Err(TransportError::Decode { encoding, reason });
        }
        let (head, body) = answered.streamed(request);
        Ok(Opened { head, stream: Some(Box::new(BodyStream { body: Some(body) })) })
    }
}

/// An answer's body as a stream: each data frame is a [`StreamFrame::Chunk`], and it ends at
/// the body's end, its first error or [`MessageStream::close`].
struct BodyStream {
    body: Option<Incoming>,
}

#[async_trait]
impl MessageStream for BodyStream {
    async fn send(&mut self, frame: OutgoingFrame) -> Result<(), TransportError> {
        let _ = frame;
        Err(TransportError::InvalidRequest { reason: "a body stream only reads; it sends nothing".to_owned() })
    }

    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>> {
        let body = self.body.as_mut()?;
        loop {
            let Some(frame) = body.frame().await else { break };
            match frame.map(hyper::body::Frame::into_data) {
                Ok(Ok(data)) if !data.is_empty() => return Some(Ok(StreamFrame::Chunk(data))),
                Ok(_) => {}
                Err(error) => {
                    self.body = None;
                    return Some(Err(TransportError::Protocol { reason: error.to_string() }));
                }
            }
        }
        self.body = None;
        None
    }

    async fn close(&mut self, code: u16) -> Result<(), TransportError> {
        let _ = code;
        self.body = None;
        Ok(())
    }
}
