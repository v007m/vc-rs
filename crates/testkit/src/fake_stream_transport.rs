use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use vc_interfaces::{
    MessageStream, Opened, OutgoingFrame, SendContext, StreamFrame, StreamMode, StreamTransport, TransportError,
};
use vc_model::{ConnectionInfo, Exchange, HttpResponse, RenderedRequest, Timing};

use crate::status;

/// What a [`FakeStreamTransport`] answers to the `n`th open (counting from 0).
pub type Opens = dyn Fn(&RenderedRequest, StreamMode, usize) -> Result<StreamAnswer, TransportError> + Send + Sync;

/// One scripted open: the opening response, and the frames its stream yields, if one opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamAnswer {
    head: HttpResponse,
    frames: Option<Vec<Result<StreamFrame, TransportError>>>,
    echo: bool,
}

impl StreamAnswer {
    /// A stream that yields `frames` in order, then ends; frames sent on it are dropped.
    #[must_use]
    pub const fn stream(head: HttpResponse, frames: Vec<Result<StreamFrame, TransportError>>) -> Self {
        Self { head, frames: Some(frames), echo: false }
    }

    /// A WebSocket that echoes what is sent and answers the text `bye` with a close 1000 `bye`.
    #[must_use]
    pub fn echo() -> Self {
        Self { head: status(101), frames: Some(Vec::new()), echo: true }
    }

    /// An answer that opens no stream.
    #[must_use]
    pub const fn refused(head: HttpResponse) -> Self {
        Self { head, frames: None, echo: false }
    }
}

/// A stream transport that answers from a script, with no network (D112).
///
/// Clones share the count of opens. Like a real transport it refuses a refused answer's body
/// over `ctx.body_limit`, and a body stream cannot send; it reports zero timings.
#[derive(Clone)]
pub struct FakeStreamTransport {
    opens: Arc<Opens>,
    calls: Arc<AtomicUsize>,
}

impl FakeStreamTransport {
    /// Answers every open with `opens(request, mode, call_index)`.
    pub fn new(
        opens: impl Fn(&RenderedRequest, StreamMode, usize) -> Result<StreamAnswer, TransportError> + Send + Sync + 'static,
    ) -> Self {
        Self { opens: Arc::new(opens), calls: Arc::new(AtomicUsize::new(0)) }
    }

    /// Answers the `n`th open with `script[n]`, and every later one with the last entry. An
    /// empty script answers 404 with no stream.
    #[must_use]
    pub fn scripted(script: Vec<Result<StreamAnswer, TransportError>>) -> Self {
        Self::new(move |_, _, call| {
            script
                .get(call)
                .or_else(|| script.last())
                .cloned()
                .unwrap_or_else(|| Ok(StreamAnswer::refused(status(404))))
        })
    }

    /// Speaks the stream contract of `vc-mock-server` (see [`crate::stream_transport_suite`]);
    /// other paths get 404.
    #[must_use]
    pub fn contract() -> Self {
        Self::new(|request, mode, _| Ok(contract(request, mode)))
    }

    /// How many opens were asked for so far.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl fmt::Debug for FakeStreamTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeStreamTransport").field("calls", &self.calls()).finish_non_exhaustive()
    }
}

#[async_trait]
impl StreamTransport for FakeStreamTransport {
    async fn open(
        &self,
        request: RenderedRequest,
        mode: StreamMode,
        ctx: &SendContext,
    ) -> Result<Opened, TransportError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let StreamAnswer { head, frames, echo } = (self.opens)(&request, mode, call)?;
        if head.body.len() as u64 > ctx.body_limit.bytes() {
            return Err(TransportError::BodyTooLarge { limit: ctx.body_limit });
        }
        let opens = mode.opens_on().matches(head.status.as_u16());
        let stream = frames.filter(|_| opens).map(|frames| {
            let incoming = frames.into_iter().collect();
            Box::new(FakeStream { mode, incoming, echo, ended: false }) as Box<dyn MessageStream>
        });
        let head =
            Exchange { request, response: head, timing: Timing::default(), connection: ConnectionInfo::default() };
        Ok(Opened { head, stream })
    }
}

/// A scripted stream: it yields its frames, and an echo stream also what is sent on it. A
/// scripted stream ends when its frames run out; an echo stream with nothing queued waits, as a
/// server that has not answered yet does, since only `send` or `close` can change that.
struct FakeStream {
    mode: StreamMode,
    incoming: VecDeque<Result<StreamFrame, TransportError>>,
    echo: bool,
    /// A close frame or an error was yielded, or the client closed: nothing more is, and sending
    /// is an error.
    ended: bool,
}

#[async_trait]
impl MessageStream for FakeStream {
    async fn send(&mut self, frame: OutgoingFrame) -> Result<(), TransportError> {
        if self.mode == StreamMode::Body {
            return Err(TransportError::InvalidRequest { reason: "a response body stream cannot send".to_owned() });
        }
        if self.ended {
            return Err(TransportError::InvalidRequest { reason: "the stream has ended".to_owned() });
        }
        if self.echo {
            let bye = frame == OutgoingFrame::Text("bye".to_owned());
            let reply = if bye { StreamFrame::Close { code: 1000, reason: "bye".to_owned() } } else { frame.into() };
            self.incoming.push_back(Ok(reply));
        }
        Ok(())
    }

    async fn next(&mut self) -> Option<Result<StreamFrame, TransportError>> {
        if self.ended {
            return None;
        }
        let Some(frame) = self.incoming.pop_front() else {
            return if self.echo { std::future::pending().await } else { None };
        };
        self.ended = matches!(frame, Ok(StreamFrame::Close { .. }) | Err(_));
        Some(frame)
    }

    async fn close(&mut self, _code: u16) -> Result<(), TransportError> {
        self.ended = true;
        Ok(())
    }
}

fn contract(request: &RenderedRequest, mode: StreamMode) -> StreamAnswer {
    let path = request.uri.path();
    if let Some(code) = path.strip_prefix("/status/") {
        return status_answer(code);
    }
    match (mode, path) {
        (StreamMode::Body, "/events") => events(request),
        (StreamMode::WebSocket, "/ws") => StreamAnswer::echo(),
        _ => StreamAnswer::refused(status(404)),
    }
}

/// The contract's `/status/{code}`: that status, with the code as a JSON body; 400 with no body
/// when `code` is not a status.
fn status_answer(code: &str) -> StreamAnswer {
    let Some(code) = code.parse().ok().filter(|code| StatusCode::from_u16(*code).is_ok()) else {
        return StreamAnswer::refused(status(400));
    };
    let mut head = status(code);
    head.headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    head.body = Bytes::from(format!(r#"{{"status":{code}}}"#));
    StreamAnswer::refused(head)
}

/// The contract's `/events?count={n}`: one chunk per event after the `last-event-id`, then an
/// error with `drop=true`.
fn events(request: &RenderedRequest) -> StreamAnswer {
    let query = request.uri.query().unwrap_or_default();
    let param = |name: &str| query.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='));
    let count: u32 = param("count").and_then(|count| count.parse().ok()).unwrap_or(0);
    let last = request.headers.get("last-event-id").and_then(|id| id.to_str().ok()?.parse().ok()).unwrap_or(0);
    let chunk = |id: u32| Ok(StreamFrame::Chunk(Bytes::from(format!("id: {id}\ndata: {id}\n\n"))));
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    let head = HttpResponse { status: StatusCode::OK, headers, body: Bytes::new(), wire_size: 0 };
    let mut frames: Vec<_> = (last + 1..=count).map(chunk).collect();
    if param("drop") == Some("true") {
        frames.push(Err(TransportError::Protocol { reason: "the connection dropped".to_owned() }));
    }
    StreamAnswer::stream(head, frames)
}

#[cfg(test)]
#[path = "fake_stream_transport_tests.rs"]
mod tests;
