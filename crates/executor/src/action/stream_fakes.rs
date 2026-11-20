//! A stream transport for the stream call tests that hangs, fails or records where they ask.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use async_trait::async_trait;
use vc_interfaces::{
    MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport, TransportError,
};
use vc_model::RenderedRequest;
use vc_testkit::{FakeStreamTransport, FakeTransport};

use crate::{HostLimits, PipelineParts};

/// Where a [`Stuck`] transport does not answer as asked.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hang {
    /// Nowhere: its streams only record the close code.
    Nowhere,
    /// A `send` on any of its streams.
    Sends,
    /// Every open after the first.
    Reopens,
    /// A `send` on any of its streams, which fails at once with [`reset`].
    Breaks,
}

/// A stream transport that hangs or fails where its [`Hang`] says, and records the code its
/// streams were last closed with.
#[derive(Clone)]
pub struct Stuck {
    streams: FakeStreamTransport,
    hang: Hang,
    closed: Arc<AtomicU16>,
}

impl Stuck {
    pub fn new(streams: FakeStreamTransport, hang: Hang) -> Self {
        Self { streams, hang, closed: Arc::new(AtomicU16::new(0)) }
    }

    /// The code a stream was last closed with; `0` when none was.
    pub fn closed(&self) -> u16 {
        self.closed.load(Ordering::SeqCst)
    }

    /// Parts whose stream port is this transport.
    pub fn parts(&self) -> PipelineParts {
        let fake = FakeTransport::scripted(vec![]);
        PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)).with_streams(Arc::new(self.clone()))
    }
}

struct StuckSend(Box<dyn MessageStream>, Stuck);

#[async_trait]
impl StreamTransport for Stuck {
    async fn open(
        &self,
        request: RenderedRequest,
        mode: StreamMode,
        ctx: &SendContext,
    ) -> Result<Opened, TransportError> {
        if self.hang == Hang::Reopens && self.streams.calls() > 0 {
            return std::future::pending().await;
        }
        let opened = self.streams.open(request, mode, ctx).await?;
        let stuck = |stream| Box::new(StuckSend(stream, self.clone())) as Box<dyn MessageStream>;
        Ok(Opened { stream: opened.stream.map(stuck), ..opened })
    }
}

#[async_trait]
impl MessageStream for StuckSend {
    async fn send(&mut self, frame: OutgoingFrame) -> Result<(), TransportError> {
        match self.1.hang {
            Hang::Breaks => Err(reset()),
            Hang::Sends => std::future::pending().await,
            Hang::Nowhere | Hang::Reopens => self.0.send(frame).await,
        }
    }

    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>> {
        self.0.next().await
    }

    async fn close(&mut self, code: u16) -> Result<(), TransportError> {
        self.1.closed.store(code, Ordering::SeqCst);
        self.0.close(code).await
    }
}

/// The error a [`Hang::Breaks`] stream's `send` fails with.
pub fn reset() -> TransportError {
    TransportError::Connect { host: "api.test".to_owned(), reason: "reset".to_owned() }
}
