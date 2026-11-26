//! The live stream RPCs (D543, D544). `CallStream` opens a call of an `sse` or `websocket` api,
//! or an ad hoc one, and streams what it reads as it reads it, masked, until it ends; closing
//! `CallStream` closes the stream. `StreamSend` sends a text on an open WebSocket, or closes it;
//! a text on an `sse` stream, which only reads, is refused and the stream stays open.
//! The open streams are known by an ID that a task of their own hands out and looks up, so no
//! state is shared; the call, which compiles the project first, runs on the blocking pool.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::call_stream_response::Event;
use vc_engine::{LiveEvent, LiveIo, LiveSend};

use crate::authoring_rpcs::call_request;
use crate::engine_api::{EngineApi, Rpcs};
use crate::{Code, ConnectError, Sink, wire_results};

/// Events that wait for a slow client before the stream waits for it.
const EVENTS: usize = 64;

/// Sends that wait for the stream before `StreamSend` waits for it.
const SENDS: usize = 16;

/// The WebSocket close code of a `StreamSend` that closes: a normal closure.
const CLOSE: u16 = 1000;

/// Serves `CallStream` and `StreamSend`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.server_stream("CallStream", call_stream).unary("StreamSend", stream_send)
}

/// The open live streams, by ID, which a task of their own keeps.
pub struct LiveStreams {
    asks: mpsc::UnboundedSender<Ask>,
}

/// What the keeper of the streams is asked.
enum Ask {
    /// Keep a new stream's sends; its ID is the answer.
    Open(mpsc::Sender<LiveSend>, oneshot::Sender<String>),
    /// The stream with this ID only reads: it takes a close but no text.
    ReadsOnly(String),
    /// The stream with this ID, while it is open.
    Find(String, oneshot::Sender<Option<Kept>>),
}

/// An open stream, as the keeper keeps it.
#[derive(Clone)]
struct Kept {
    sends: mpsc::Sender<LiveSend>,
    reads_only: bool,
}

impl LiveStreams {
    /// The streams, kept by a task on the current Tokio runtime; outside one, none opens.
    pub fn spawn() -> Self {
        let (asks, inbox) = mpsc::unbounded_channel();
        if let Ok(runtime) = Handle::try_current() {
            drop(runtime.spawn(keep(inbox)));
        }
        Self { asks }
    }

    /// Keeps `sends` as an open stream's; its ID.
    async fn open(&self, sends: mpsc::Sender<LiveSend>) -> Result<String, ConnectError> {
        let unavailable = || ConnectError::new(Code::Unavailable, "the server keeps no live streams");
        let (reply, id) = oneshot::channel();
        self.asks.send(Ask::Open(sends, reply)).map_err(|_| unavailable())?;
        id.await.map_err(|_| unavailable())
    }

    /// Marks the open stream `stream` as one that only reads. The keeper takes asks in order,
    /// so a `find` asked after this returns finds it marked.
    fn reads_only(&self, stream: &str) {
        drop(self.asks.send(Ask::ReadsOnly(stream.to_owned())));
    }

    /// The open stream `stream`.
    async fn find(&self, stream: String) -> Option<Kept> {
        let (reply, found) = oneshot::channel();
        self.asks.send(Ask::Find(stream, reply)).ok()?;
        found.await.ok().flatten()
    }
}

/// Answers `asks` until the API that asks them has gone. A stream whose call has returned is
/// forgotten at the next ask.
async fn keep(mut asks: mpsc::UnboundedReceiver<Ask>) {
    let mut open: HashMap<String, Kept> = HashMap::new();
    let mut opened: u64 = 0;
    while let Some(ask) = asks.recv().await {
        open.retain(|_, kept| !kept.sends.is_closed());
        match ask {
            Ask::Open(sends, reply) => {
                opened += 1;
                let id = format!("live-{opened}");
                drop(open.insert(id.clone(), Kept { sends, reads_only: false }));
                drop(reply.send(id));
            }
            Ask::ReadsOnly(stream) => {
                if let Some(kept) = open.get_mut(&stream) {
                    kept.reads_only = true;
                }
            }
            Ask::Find(stream, reply) => drop(reply.send(open.get(&stream).cloned())),
        }
    }
}

/// Opens `request`'s call as a live stream and streams its events to `sink` until it ends or
/// the client goes, which closes it.
async fn call_stream(
    api: Arc<EngineApi>,
    request: wire::CallStreamRequest,
    sink: Sink<wire::CallStreamResponse>,
) -> Result<(), ConnectError> {
    let call = call_request(&api, request.call.unwrap_or_default())?;
    let (asks, sends) = mpsc::channel(SENDS);
    let stream = api.live().open(asks).await?;
    let (events, told) = mpsc::channel(EVENTS);
    let io = LiveIo::new(events, sends);
    let running = api.blocking(move |engine| Handle::current().block_on(engine.call_stream(&call, io)));
    let (ran, relayed) = tokio::join!(running, relay(told, &sink, (api.live(), &stream)));
    ran?;
    relayed
}

/// Sends the stream's events to `sink` until it ends, marking it in `live` as one that only
/// reads before the client hears it opened. When the client goes, or an event cannot be sent,
/// it stops reading, which closes the stream.
async fn relay(
    mut told: mpsc::Receiver<LiveEvent>,
    sink: &Sink<wire::CallStreamResponse>,
    (live, stream): (&LiveStreams, &str),
) -> Result<(), ConnectError> {
    loop {
        let event = tokio::select! {
            event = told.recv() => event,
            () = sink.closed() => None,
        };
        let Some(event) = event else { return Ok(()) };
        if matches!(event, LiveEvent::Opened { sends: false, .. }) {
            live.reads_only(stream);
        }
        if let Some(event) = wire_event(event, stream) {
            sink.send(&wire::CallStreamResponse { event: Some(event) }).await?;
        }
    }
}

/// `event` of the stream `stream` in its wire form.
fn wire_event(event: LiveEvent, stream: &str) -> Option<Event> {
    Some(match event {
        LiveEvent::Opened { run_id, names, action, head, sends } => Event::Opened(wire::StreamOpened {
            stream: stream.to_owned(),
            run_id,
            head: Some(wire_results::exchange(&names, action, &head)),
            sends,
        }),
        LiveEvent::Message { outgoing, at_ms, fields } => Event::Message(wire::StreamMessage {
            direction: (if outgoing { "out" } else { "in" }).to_owned(),
            at_ms,
            json: Value::Object(fields).to_string(),
        }),
        LiveEvent::Ended { reason, failed, code } => {
            Event::Ended(wire::StreamEnded { reason, failed, code: code.map(u32::from) })
        }
        _ => return None,
    })
}

/// Sends `request.text` on the open stream `request.stream`, or closes it when `request.close`.
/// A text on a stream that only reads fails with `failed_precondition` and leaves it open.
async fn stream_send(
    api: Arc<EngineApi>,
    request: wire::StreamSendRequest,
) -> Result<wire::StreamSendResponse, ConnectError> {
    let send = match (request.close, request.text) {
        (true, _) => LiveSend::Close(CLOSE),
        (false, Some(text)) => LiveSend::Text(text),
        (false, None) => return Err(ConnectError::new(Code::InvalidArgument, "send a `text` or `close: true`")),
    };
    let gone = ConnectError::new(Code::NotFound, format!("no live stream `{}` is open", request.stream));
    let kept = api.live().find(request.stream.clone()).await.ok_or_else(|| gone.clone())?;
    if kept.reads_only && matches!(send, LiveSend::Text(_)) {
        let reads = format!("the stream `{}` only reads; it sends nothing", request.stream);
        return Err(ConnectError::new(Code::FailedPrecondition, reads));
    }
    kept.sends.send(send).await.map_err(|_| gone)?;
    Ok(wire::StreamSendResponse {})
}

#[cfg(test)]
#[path = "live_rpcs_tests.rs"]
mod tests;
