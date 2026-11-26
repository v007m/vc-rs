//! The web UI Client's live stream (D543, D544): a call of an `sse` or `websocket` api, an api
//! of the project's or an ad hoc one, opened as `vc call` opens it and kept open while its
//! caller reads, sends and closes. Everything it tells the caller is masked, and it is recorded
//! in the history as an `adhoc` run with its opening exchange.

use std::sync::Arc;

use serde_json::{Map, Value};
use tokio::sync::mpsc;
use vc_executor::{LiveEnd, LiveRead, LiveStream, Redactor};
use vc_interfaces::Clock;
use vc_model::{ActionId, Exchange, Names};

use crate::adhoc;
use crate::call::{Ready, sent};
use crate::{CallRequest, Engine, EngineError};

/// What the caller of a live stream asks of it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LiveSend {
    /// A WebSocket text message, sent as written: a `${…}` in it is not rendered.
    Text(String),
    /// Close the stream with this WebSocket close code.
    Close(u16),
}

/// What a live stream tells its caller, in order: `Opened`, its `Message`s, then `Ended`. A
/// call that fails before it opens tells only `Ended`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum LiveEvent {
    /// The stream opened.
    Opened {
        /// The history run that records it; `0` when there is no history.
        run_id: u64,
        /// What names the call's IDs.
        names: Names,
        /// The action it sent.
        action: ActionId,
        /// The opening request and response, masked.
        head: Box<Exchange>,
        /// Whether the stream takes sends: a `websocket` one does, an `sse` one only reads
        /// (D544).
        sends: bool,
    },
    /// A message received or sent.
    Message {
        /// Whether the caller sent it.
        outgoing: bool,
        /// Milliseconds since the stream opened.
        at_ms: u64,
        /// Its fields (D379), masked.
        fields: Map<String, Value>,
    },
    /// The stream ended; nothing follows.
    Ended {
        /// Why, in words, masked.
        reason: String,
        /// Whether it ended on a failure rather than a close.
        failed: bool,
        /// The WebSocket close code the server sent, if any.
        code: Option<u16>,
    },
}

/// The two ends a live stream talks through: it tells `events` and reads `sends`. Dropping the
/// receiver of `events`, or the sender of `sends`, closes the stream.
#[derive(Debug)]
#[non_exhaustive]
pub struct LiveIo {
    /// Where the stream's events go.
    pub events: mpsc::Sender<LiveEvent>,
    /// What the caller asks of it.
    pub sends: mpsc::Receiver<LiveSend>,
}

impl LiveIo {
    /// The ends `events` and `sends`.
    #[must_use]
    pub const fn new(events: mpsc::Sender<LiveEvent>, sends: mpsc::Receiver<LiveSend>) -> Self {
        Self { events, sends }
    }
}

impl Engine {
    /// Opens `request`'s call as a live stream and talks through `io` until the server, the
    /// caller or the connection ends it; then records it and returns. Its plugins have exited
    /// by then.
    ///
    /// # Errors
    ///
    /// As [`Engine::call`], before anything opens; a failure after that is the `Ended` event.
    pub async fn call_stream(&self, request: &CallRequest, io: LiveIo) -> Result<(), EngineError> {
        let request = adhoc::resolved(request)?;
        let ready = self.ready(request.as_ref()).await?;
        let names = Names::new(Arc::clone(&ready.prepared.program), &ready.prepared.plan);
        let (head, passed) = ready.plugins.around(live((&ready, &names), io, &*self.clock)).await;
        let Ready { action, admitted, recording, .. } = ready;
        drop(self.recorded(recording, (&names, action), (head.as_ref(), passed)).await);
        self.settle(admitted, passed).await
    }
}

/// Opens `ready`'s call, which `names` names, and talks through `io`: the masked head it opened
/// with, and whether it ended without a failure.
async fn live((ready, names): (&Ready, &Names), io: LiveIo, clock: &dyn Clock) -> (Option<Exchange>, bool) {
    let LiveIo { events, sends: mut asked } = io;
    let redactor = &*ready.cx.redactor;
    let (head, end) = match ready.cx.executors.open_live(sent(&ready.cx, ready.action, ready.given.clone())).await {
        Ok(stream) => {
            let head = redactor.exchange(stream.head().clone()).await;
            let (run_id, action) = (ready.run_id, ready.action);
            let sends = stream.sends();
            let opened =
                LiveEvent::Opened { run_id, names: names.clone(), action, head: Box::new(head.clone()), sends };
            drop(events.send(opened).await);
            let talk = Talk { stream, events: &events, redactor, clock, started_ms: clock.now_ms() };
            (Some(head), talk.converse(&mut asked).await)
        }
        Err(failure) => (None, LiveEnd::new(failure.message, true)),
    };
    let ended =
        LiveEvent::Ended { reason: redactor.masked_text(&end.reason).await, failed: end.failed, code: end.code };
    drop(events.send(ended).await);
    (head, !end.failed)
}

/// What woke the stream.
enum Step {
    Read(LiveRead),
    Asked(Option<LiveSend>),
    Left,
}

/// One open live stream and the caller it talks to.
struct Talk<'a> {
    stream: LiveStream,
    events: &'a mpsc::Sender<LiveEvent>,
    redactor: &'a Redactor,
    clock: &'a dyn Clock,
    started_ms: u64,
}

impl<'a> Talk<'a> {
    /// Reads, sends and closes as the stream and the caller say, until it ends.
    async fn converse(mut self, sends: &mut mpsc::Receiver<LiveSend>) -> LiveEnd {
        let events = self.events;
        loop {
            let step = tokio::select! {
                read = self.stream.next() => Step::Read(read),
                asked = sends.recv() => Step::Asked(asked),
                () = events.closed() => Step::Left,
            };
            if let Some(end) = self.step(step).await {
                return end;
            }
        }
    }

    /// Acts on `step`; how the stream ended, once it has.
    async fn step(&mut self, step: Step) -> Option<LiveEnd> {
        match step {
            Step::Read(LiveRead::Message(message)) => {
                self.tell(false, message.fields).await;
                None
            }
            Step::Read(LiveRead::Ended(end)) => Some(end),
            Step::Read(_) => Some(LiveEnd::new("the stream ended", false)),
            Step::Asked(Some(LiveSend::Text(text))) => self.send(text).await,
            Step::Asked(Some(LiveSend::Close(code))) => Some(self.close(code, "the client closed the stream").await),
            Step::Asked(None) | Step::Left => Some(self.close(1000, "the client left").await),
        }
    }

    /// Sends `text` and tells it as an outgoing message; a failed send ends the stream. A stream
    /// that only reads (`sse`) ignores it and stays open: its caller refuses such a send (D544).
    async fn send(&mut self, text: String) -> Option<LiveEnd> {
        if !self.stream.sends() {
            return None;
        }
        let data = serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()));
        let text = Value::String(text);
        if let Err(failure) = self.stream.send(&text).await {
            return Some(LiveEnd::new(failure.message, true));
        }
        let fields = [("type", Value::from("text")), ("data", data), ("text", text)];
        self.tell(true, fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect()).await;
        None
    }

    /// Closes the stream with `code`, for `reason`; the caller asked, so it is no failure.
    async fn close(&mut self, code: u16, reason: &str) -> LiveEnd {
        drop(self.stream.close(code).await);
        LiveEnd::new(reason, false)
    }

    /// Tells the caller of a message, masked. The telling holds no borrow of the stream, which
    /// is not `Sync`, so the call's future stays `Send`.
    fn tell(&self, outgoing: bool, fields: Map<String, Value>) -> impl Future<Output = ()> + Send + 'a {
        let (events, redactor, at_ms) =
            (self.events, self.redactor, self.clock.now_ms().saturating_sub(self.started_ms));
        async move {
            let Value::Object(fields) = redactor.masked(Value::Object(fields)).await else { return };
            drop(events.send(LiveEvent::Message { outgoing, at_ms, fields }).await);
        }
    }
}

#[cfg(test)]
#[path = "live_call_tests.rs"]
mod tests;
