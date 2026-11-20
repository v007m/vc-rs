//! A stream call (D377–D385, D421–D424): renders its `send` items, opens the stream through the
//! request pipeline, then sends and reads against the call's deadline, judging each message
//! against the `receive` the [`ExchangeCursor`] waits on.

use std::slice;
use std::time::Duration;

use http::{HeaderName, HeaderValue};
use serde_json::Value;
use tokio::time::{self, Instant};
use vc_diagnostics::Code;
use vc_interfaces::{MessageStream, StreamFrame};
use vc_model::{
    Action, ActionId, Api, CheckKind, CheckResult, Exchange, ExchangeItem, ExchangeKind, Failure, Matcher, Origin,
    SourceSpan, StreamMessage, TemplateValue,
};

use super::exchange::{ExchangeCursor, Fed, Item, Unmet};
use super::flight::{Answer, Flight, Halted};
use super::request;
use super::stream::{Decoder, StreamRuntime};
use crate::check::Judge;
use crate::{Call, Opening, Render};

/// The header a reconnect resumes with (D385).
const LAST_EVENT_ID: HeaderName = HeaderName::from_static("last-event-id");
/// The wait before a reconnect when the server sent no `retry`.
const RECONNECT_WAIT: Duration = Duration::from_secs(1);

/// The stream call asked for: the step and the api it calls.
#[derive(Clone, Copy)]
pub struct Asked<'a> {
    /// The step and its id.
    pub action: (ActionId, &'a Action),
    /// The api it calls.
    pub api: &'a Api,
}

impl<'a> Asked<'a> {
    /// The exchange run: the step's own (its test's included, D417), else the api's.
    fn items(self) -> &'a [ExchangeItem] {
        self.action.1.exchange.as_deref().unwrap_or(&self.api.exchange)
    }

    /// Where that exchange was written.
    const fn origin(self) -> Origin {
        if self.action.1.exchange.is_some() { Origin::Action(self.action.0) } else { Origin::Api(self.api.id) }
    }
}

/// Runs the stream call `call` asked by `asked` through `flight`.
///
/// # Errors
///
/// When the open fails, a `send` does not render or go out, the messages pass `body_limit`, or
/// an `afterResponse` hook fails.
pub async fn run(flight: &Flight, render: &mut Render<'_>, (call, asked): (Call, Asked<'_>)) -> Result<Answer, Halted> {
    let runtime = flight.runtime().stream().ok_or_else(|| Failure::new("the api type reads no stream"))?;
    let mut session = Session::new(runtime, asked, (render, &call))?;
    let (head, mut opening) = held(flight.open(call.clone()).await?);
    session.deadline = Instant::now() + call.timeout;
    let halted = |failure| Halted::with(failure, head.clone());
    let mut ended = session.read((flight, render), opening.as_mut()).await.map_err(halted)?;
    if session.resumes(&ended) {
        drop(opening.take());
        opening = session.reopen(flight, call).await;
        if opening.is_some() {
            ended = session.read((flight, render), opening.as_mut()).await.map_err(halted)?;
        }
    }
    close(opening.as_mut(), (session.deadline, &ended)).await;
    let answer = session.answer(&Judge::new(flight.matchers(), &*render), (head, &ended));
    let after = flight.after(&answer.exchange, || answer.response(flight.runtime())).await;
    after.map_err(|failure| Halted::with(failure, answer.exchange.clone()))?;
    Ok(answer)
}

/// How a read of one connection ended.
#[derive(Debug)]
enum Ended {
    /// The exchange is done.
    Done,
    /// The stream did not open, or opened as something its type does not read (V5131).
    Refused(String),
    /// The server ended the stream.
    Closed,
    /// The connection broke; it ends the read as a close does (D424).
    Dropped(String),
    /// The call's time limit passed.
    TimedOut(Duration),
}

impl Ended {
    fn reason(&self) -> String {
        match self {
            Self::Done | Self::Refused(_) => "the exchange ended".to_owned(),
            Self::Closed => "the stream closed".to_owned(),
            Self::Dropped(error) => format!("the connection dropped ({error})"),
            Self::TimedOut(limit) => format!("the {limit:?} time limit passed"),
        }
    }
}

/// One exchange item as the driver reads it.
struct Shape<'a> {
    send: Option<&'a TemplateValue>,
    entries: &'a [Matcher],
    grouped: bool,
}

fn shape(kind: &ExchangeKind) -> Shape<'_> {
    match kind {
        ExchangeKind::Send(value) => Shape { send: Some(value), entries: &[], grouped: false },
        ExchangeKind::Receive(matcher) => Shape { send: None, entries: slice::from_ref(matcher), grouped: false },
        ExchangeKind::Group(matchers) => Shape { send: None, entries: matchers, grouped: true },
    }
}

/// One stream call's state across its connections.
struct Session<'a> {
    runtime: &'a dyn StreamRuntime,
    asked: Asked<'a>,
    shapes: Vec<Shape<'a>>,
    cursor: ExchangeCursor,
    decoder: Box<dyn Decoder>,
    sends: Vec<Option<Value>>,
    items: Vec<Value>,
    messages: Vec<StreamMessage>,
    kept: u64,
    limit: u64,
    timeout: Duration,
    deadline: Instant,
}

impl<'a> Session<'a> {
    /// A session for `asked`, its sends rendered before anything opens (D377).
    fn new(
        runtime: &'a dyn StreamRuntime,
        asked: Asked<'a>,
        (render, call): (&Render<'_>, &Call),
    ) -> Result<Self, Failure> {
        let shapes: Vec<_> = asked.items().iter().map(|item| shape(&item.kind)).collect();
        let rendered = shapes.iter().map(|shape| shape.send.map(|value| render.value(value)).transpose());
        let sends = rendered.collect::<Result<Vec<_>, _>>().map_err(request::failed)?;
        let plan = shapes
            .iter()
            .map(|shape| if shape.send.is_some() { Item::Send } else { Item::Receive(shape.entries.len()) });
        let cursor = ExchangeCursor::new(plan.collect());
        Ok(Self {
            runtime,
            asked,
            items: vec![Value::Null; shapes.len()],
            shapes,
            cursor,
            decoder: runtime.decoder(),
            sends,
            messages: Vec::new(),
            kept: 0,
            limit: call.ctx.body_limit.bytes(),
            timeout: call.timeout,
            deadline: Instant::now() + call.timeout,
        })
    }

    /// Sends and reads on `opening` until the exchange is done or the stream ends.
    async fn read(
        &mut self,
        (flight, render): (&Flight, &mut Render<'_>),
        opening: Option<&mut Opening>,
    ) -> Result<Ended, Failure> {
        let Some(opening) = opening else { return Ok(Ended::Closed) };
        if let Some(reason) = self.refusal(opening) {
            return Ok(Ended::Refused(reason));
        }
        let Some(stream) = opening.stream.as_deref_mut() else { return Ok(Ended::Closed) };
        loop {
            self.send_due(stream).await?;
            if self.cursor.is_done() && !self.cursor.reads_to_end() {
                return Ok(Ended::Done);
            }
            let frame = match time::timeout_at(self.deadline, stream.next()).await {
                Err(_) => return Ok(Ended::TimedOut(self.timeout)),
                Ok(None) => return Ok(Ended::Closed),
                Ok(Some(Err(error))) => return Ok(Ended::Dropped(error.to_string())),
                Ok(Some(Ok(frame))) => frame,
            };
            self.frame(&Judge::new(flight.matchers(), &*render), frame)?;
        }
    }

    /// Why `opening` is not a stream this type reads, if it is not.
    fn refusal(&self, opening: &Opening) -> Option<String> {
        match opening.stream {
            None => Some(format!("the server answered {}", opening.head.response.status.as_u16())),
            Some(_) => self.runtime.opened(&opening.head).err(),
        }
    }

    /// Sends each `send` item the cursor reached.
    async fn send_due(&mut self, stream: &mut dyn MessageStream) -> Result<(), Failure> {
        while let Some(item) = self.cursor.next_send() {
            let value = self.sends.get_mut(item).and_then(Option::take).unwrap_or(Value::Null);
            let frame = self.runtime.encode(&value);
            if let Some(slot) = self.items.get_mut(item) {
                *slot = value;
            }
            let sent = time::timeout_at(self.deadline, stream.send(frame)).await.unwrap_or(Ok(()));
            sent.map_err(|error| Failure::new(error.to_string()).with_code(error.code().to_string()))?;
        }
        Ok(())
    }

    /// Decodes `frame` and feeds each message it completes to the cursor.
    fn frame(&mut self, judge: &Judge<'_>, frame: StreamFrame) -> Result<(), Failure> {
        self.kept = self.kept.saturating_add(size(&frame));
        if self.kept > self.limit {
            let message = format!("the stream's messages are larger than {} bytes", self.limit);
            return Err(Failure::new(message).with_code(Code::V5105.to_string()));
        }
        for message in self.decoder.push(frame) {
            self.take(judge, message);
        }
        Ok(())
    }

    /// Keeps `message`, and fills the item it completes.
    fn take(&mut self, judge: &Judge<'_>, message: StreamMessage) {
        let fits = self.fits(judge, &Value::Object(message.fields.clone()));
        self.messages.push(message);
        let Fed::Met { item, pairs } = self.cursor.feed(&fits) else { return };
        let mut found = pairs.iter().filter_map(|&at| self.messages.get(at)).map(object);
        let grouped = self.shapes.get(item).is_some_and(|shape| shape.grouped);
        let value = if grouped { Value::Array(found.collect()) } else { found.next().unwrap_or_default() };
        if let Some(slot) = self.items.get_mut(item) {
            *slot = value;
        }
    }

    /// The entries of the waiting group that `message` passes.
    fn fits(&self, judge: &Judge<'_>, message: &Value) -> Vec<usize> {
        let Some(shape) = self.cursor.receiving().and_then(|item| self.shapes.get(item)) else { return Vec::new() };
        let entries = shape.entries.iter().enumerate();
        entries.filter(|(_, matcher)| judge.matches(matcher, Some(message)) == Ok(true)).map(|(at, _)| at).collect()
    }

    /// Whether `ended` leaves the exchange undone on a stream that reconnects (D385).
    fn resumes(&self, ended: &Ended) -> bool {
        let reconnects = self.asked.api.stream.is_some_and(|stream| stream.reconnect);
        reconnects && matches!(ended, Ended::Closed | Ended::Dropped(_)) && !self.cursor.is_done()
    }

    /// Opens the stream once more after the server's `retry`, resuming from its last `id`.
    async fn reopen(&mut self, flight: &Flight, mut call: Call) -> Option<Opening> {
        let resume = self.decoder.resume();
        let wait = resume.retry.unwrap_or(RECONNECT_WAIT);
        if Instant::now() + wait > self.deadline {
            return None;
        }
        time::sleep(wait).await;
        if let Some(id) = resume.last_id.and_then(|id| HeaderValue::from_str(&id).ok()) {
            drop(call.request.headers.insert(LAST_EVENT_ID, id));
        }
        let opening = time::timeout_at(self.deadline, flight.open(call)).await.ok()?.ok()?;
        self.decoder = self.runtime.decoder();
        self.refusal(&opening).is_none().then_some(opening)
    }

    /// The call's answer: the head with every message, the `exchange` items and the failure.
    fn answer(self, judge: &Judge<'_>, (head, ended): (Exchange, &Ended)) -> Answer {
        let failed = match ended {
            Ended::Refused(reason) => self.refused(&head, reason),
            _ => self.unmet(judge, ended),
        };
        let exchange = Exchange { messages: self.messages, ..head };
        Answer { exchange, items: Some(self.items), checks: failed.into_iter().collect() }
    }

    /// V5131 at the first `receive`; with none, at the step, and only when the head itself
    /// succeeded, as a failed head already fails its status check.
    fn refused(&self, head: &Exchange, reason: &str) -> Option<CheckResult> {
        let receive = self.asked.items().iter().zip(&self.shapes).find(|(_, shape)| shape.send.is_none());
        let succeeded = self.asked.api.success_status().matches(head.response.status.as_u16());
        let at = match receive {
            Some((item, _)) => (self.asked.origin(), item.span),
            None if succeeded => (Origin::Action(self.asked.action.0), self.asked.action.1.span),
            None => return None,
        };
        Some(failed(at, Code::V5131, format!("the stream did not open: {reason}")))
    }

    /// V5130 at the `receive` still waiting, with what it expected and the messages since the
    /// last match.
    fn unmet(&self, judge: &Judge<'_>, ended: &Ended) -> Option<CheckResult> {
        let Unmet { item, entries, since } = self.cursor.unmet()?;
        let (written, shape) = (self.asked.items().get(item)?, self.shapes.get(item)?);
        let mut expected = entries.iter().filter_map(|&entry| shape.entries.get(entry));
        let mut expected = expected.by_ref().map(|matcher| judge.expected(matcher).unwrap_or_default());
        let reason = format!("no message matched before {}", ended.reason());
        let mut result = failed((self.asked.origin(), written.span), Code::V5130, reason);
        result.expected =
            if shape.grouped { Value::Array(expected.collect()) } else { expected.next().unwrap_or_default() };
        result.actual = Some(self.messages.get(since..).unwrap_or_default().iter().map(object).collect());
        Some(result)
    }
}

/// The opening's head, and the opening held for reading.
fn held(opening: Opening) -> (Exchange, Option<Opening>) {
    (opening.head.clone(), Some(opening))
}

/// Closes a stream whose exchange is done (a WebSocket close 1000), before `deadline`.
async fn close(opening: Option<&mut Opening>, (deadline, ended): (Instant, &Ended)) {
    let Some(stream) = opening.and_then(|opening| opening.stream.as_deref_mut()) else { return };
    if matches!(ended, Ended::Done) {
        drop(time::timeout_at(deadline, stream.close(1000)).await);
    }
}

/// A failed messages check at `at`.
fn failed((origin, span): (Origin, SourceSpan), code: Code, error: String) -> CheckResult {
    let (expected, actual, diff) = (Value::Null, None, None);
    let (target, code) = (None, Some(code.to_string()));
    CheckResult {
        kind: CheckKind::Messages,
        target,
        origin,
        span,
        passed: false,
        expected,
        actual,
        diff,
        error: Some(error),
        code,
    }
}

fn object(message: &StreamMessage) -> Value {
    Value::Object(message.fields.clone())
}

/// The bytes `frame` carries, as `body_limit` counts them.
fn size(frame: &StreamFrame) -> u64 {
    let len = match frame {
        StreamFrame::Chunk(bytes) | StreamFrame::Binary(bytes) => bytes.len(),
        StreamFrame::Text(text) => text.len(),
        StreamFrame::Close { reason, .. } => reason.len(),
    };
    u64::try_from(len).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "stream_call_tests.rs"]
mod tests;
