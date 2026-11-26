//! A live stream (D543): a call of an `sse` or `websocket` api opened as the executor opens a
//! stream call, then read and written by its caller for as long as it likes. It has no `exchange`
//! script, no deadline and no size cap, and its `afterResponse` hooks do not run.

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::time;
use vc_interfaces::{StreamFrame, TransportError};
use vc_model::{Exchange, Failure, StreamMessage};

use super::ActionRequest;
use super::call::{Begun, CallExecutor};
use super::protocol::ProtocolRuntime;
use super::stream::{Decoder, StreamRuntime};
use super::stream_call::refusal;
use crate::Opening;

/// How long a close waits for the server's close.
const CLOSE_WAIT: Duration = Duration::from_secs(5);

/// An open stream: its opening response, and the messages and sends that follow it.
pub struct LiveStream {
    opening: Opening,
    runtime: Arc<dyn ProtocolRuntime>,
    decoder: Box<dyn Decoder>,
    pending: VecDeque<StreamMessage>,
    refused: Option<String>,
    code: Option<u16>,
}

/// What a read of a live stream gave.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum LiveRead {
    /// A message, decoded as the api's type decodes it (D379).
    Message(StreamMessage),
    /// The stream ended; every later read ends it again.
    Ended(LiveEnd),
}

/// How a live stream ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LiveEnd {
    /// Why, in words.
    pub reason: String,
    /// Whether it ended on a failure rather than a close.
    pub failed: bool,
    /// The WebSocket close code the server sent, if any.
    pub code: Option<u16>,
}

impl LiveEnd {
    /// An end for `reason`.
    #[must_use]
    pub fn new(reason: impl Into<String>, failed: bool) -> Self {
        Self { reason: reason.into(), failed, code: None }
    }
}

impl fmt::Debug for LiveStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveStream").field("head", &self.opening.head.response.status).finish_non_exhaustive()
    }
}

impl CallExecutor {
    /// Opens `request`'s call as a live stream, through the pipeline with its auth and
    /// `beforeRequest` hooks. An answer that opened no stream its type reads still opens: its
    /// first read ends it, failed.
    pub(super) async fn live(&self, request: &ActionRequest) -> Result<LiveStream, Failure> {
        let Begun { runtime, call, trip, .. } = self.begin(request)?;
        let decoder = streaming(&*runtime)?.decoder();
        let opening = trip.open(call).await?;
        let refused = refusal(streaming(&*runtime)?, &opening);
        Ok(LiveStream { opening, runtime, decoder, pending: VecDeque::new(), refused, code: None })
    }
}

impl LiveStream {
    /// The opening response, as sent and received; not yet masked.
    #[must_use]
    pub const fn head(&self) -> &Exchange {
        &self.opening.head
    }

    /// The next message, or how the stream ended. It waits only on the stream itself, so a
    /// read dropped in a `select!` loses no message this stream had decoded.
    pub async fn next(&mut self) -> LiveRead {
        loop {
            if let Some(message) = self.pending.pop_front() {
                return LiveRead::Message(message);
            }
            if let Some(reason) = self.refused.take() {
                self.opening.stream = None;
                return LiveRead::Ended(LiveEnd::new(format!("the stream did not open: {reason}"), true));
            }
            let Some(stream) = self.opening.stream.as_deref_mut() else { return self.closed() };
            match stream.next().await {
                None => return self.closed(),
                Some(Err(error)) => return self.dropped(&error),
                Some(Ok(frame)) => self.took(frame),
            }
        }
    }

    /// Sends `value` as the api's type encodes a `send` (a string as its text).
    ///
    /// # Errors
    ///
    /// When the stream has ended, cannot send (an `sse` stream) or the send failed.
    pub async fn send(&mut self, value: &Value) -> Result<(), Failure> {
        let frame = streaming(&*self.runtime)?.encode(value);
        let stream = self.opening.stream.as_deref_mut().ok_or_else(|| Failure::new("the stream has ended"))?;
        stream.send(frame).await.map_err(|error| failure(&error))
    }

    /// Ends the stream: a WebSocket sends a close with `code` and waits a little for the
    /// server's; a body stream is dropped.
    ///
    /// # Errors
    ///
    /// When the close could not be sent or answered.
    pub async fn close(&mut self, code: u16) -> Result<(), Failure> {
        let Some(mut stream) = self.opening.stream.take() else { return Ok(()) };
        match time::timeout(CLOSE_WAIT, stream.close(code)).await {
            Ok(closed) => closed.map_err(|error| failure(&error)),
            Err(_) => Err(Failure::new(format!("the server did not answer the close within {CLOSE_WAIT:?}"))),
        }
    }

    /// Decodes `frame`, keeping a close's code.
    fn took(&mut self, frame: StreamFrame) {
        if let StreamFrame::Close { code, .. } = &frame {
            self.code = Some(*code);
        }
        self.pending.extend(self.decoder.push(frame));
    }

    fn closed(&mut self) -> LiveRead {
        self.opening.stream = None;
        LiveRead::Ended(LiveEnd { code: self.code, ..LiveEnd::new("the stream closed", false) })
    }

    fn dropped(&mut self, error: &TransportError) -> LiveRead {
        self.opening.stream = None;
        LiveRead::Ended(LiveEnd::new(format!("the connection dropped ({error})"), true))
    }
}

/// The stream side of `runtime`; a type that reads no stream opens no live stream.
fn streaming(runtime: &dyn ProtocolRuntime) -> Result<&dyn StreamRuntime, Failure> {
    runtime.stream().ok_or_else(|| Failure::new("only a call of an `sse` or `websocket` api opens a live stream"))
}

fn failure(error: &TransportError) -> Failure {
    Failure::new(error.to_string()).with_code(error.code().to_string())
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
