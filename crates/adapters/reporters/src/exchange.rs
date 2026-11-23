//! Exchanges as the history and the reports keep them: text headers and bodies, the bodies cut at
//! `history.maxBodySize` on a character boundary, and a stream's messages while they fit in it.
//!
//! The exchange arrives already masked by the redactor, whole; cutting only after that
//! means a cut can never split a secret before it is masked.

use std::time::Duration;

use http::HeaderMap;
use serde_json::Value;
use vc_model::{Bodies, Exchange, StreamMessage, Timing, cut_body};

use crate::view::{ExchangeView, TimingView};

/// What the recorder keeps of each exchange.
#[derive(Clone, Copy, Debug)]
pub struct Keeping {
    pub bodies: Bodies,
    /// The most bytes of a body kept.
    pub max_body: usize,
}

impl Keeping {
    /// Whether bodies are kept for an exchange of something that `failed` or not.
    pub const fn bodies_of(self, failed: bool) -> bool {
        match self.bodies {
            Bodies::None => false,
            Bodies::Failures => failed,
            Bodies::All => true,
        }
    }
}

/// `exchange` with its bodies cut at `max_body` bytes, and its messages too; the caller names
/// the action and api.
pub fn view(exchange: &Exchange, max_body: usize) -> ExchangeView {
    let (request_body, request_cut) = cut_body(&exchange.request.body, max_body);
    let (response_body, response_cut) = cut_body(&exchange.response.body, max_body);
    let (messages, messages_cut) = messages(&exchange.messages, max_body);
    ExchangeView {
        action: String::new(),
        api: None,
        method: exchange.request.method.as_str().to_owned(),
        url: exchange.request.uri.to_string(),
        status: exchange.response.status.as_u16(),
        timing: timing(&exchange.timing),
        request_headers: headers(&exchange.request.headers),
        response_headers: headers(&exchange.response.headers),
        request_body,
        response_body,
        messages,
        truncated: request_cut || response_cut || messages_cut,
    }
}

/// The same exchange without its bodies and messages.
pub fn without_bodies(view: ExchangeView) -> ExchangeView {
    ExchangeView { request_body: None, response_body: None, messages: Vec::new(), truncated: false, ..view }
}

/// `messages` as objects, in order, while their JSON text fits in `max_body` bytes; whether any
/// were left out. A message is kept whole or not at all.
fn messages(messages: &[StreamMessage], max_body: usize) -> (Vec<Value>, bool) {
    let mut room = max_body;
    let mut kept = Vec::new();
    for message in messages {
        let value = Value::Object(message.fields.clone());
        let Some(left) = room.checked_sub(value.to_string().len()) else { return (kept, true) };
        room = left;
        kept.push(value);
    }
    (kept, false)
}

fn headers(map: &HeaderMap) -> Vec<(String, String)> {
    let text = |value: &http::HeaderValue| String::from_utf8_lossy(value.as_bytes()).into_owned();
    map.iter().map(|(name, value)| (name.as_str().to_owned(), text(value))).collect()
}

fn timing(timing: &Timing) -> TimingView {
    TimingView {
        dns: timing.dns.map(millis),
        connect: timing.connect.map(millis),
        tls: timing.tls.map(millis),
        first_byte: millis(timing.first_byte),
        download: millis(timing.download),
        total: millis(timing.total),
    }
}

fn millis(time: Duration) -> f64 {
    time.as_secs_f64() * 1_000.0
}

#[cfg(test)]
#[path = "exchange_tests.rs"]
mod tests;
