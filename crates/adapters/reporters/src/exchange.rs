//! Exchanges as the history and the reports keep them: text headers and bodies, the bodies cut at
//! `history.maxBodySize` on a character boundary.
//!
//! The exchange arrives already masked by the redactor, whole; cutting only after that
//! means a cut can never split a secret before it is masked.

use std::time::Duration;

use http::HeaderMap;
use vc_model::{Bodies, Exchange, Timing};

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

/// `exchange` with its bodies cut at `max_body` bytes; the caller names the action and api.
pub fn view(exchange: &Exchange, max_body: usize) -> ExchangeView {
    let (request_body, request_cut) = cut(&exchange.request.body, max_body);
    let (response_body, response_cut) = cut(&exchange.response.body, max_body);
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
        truncated: request_cut || response_cut,
    }
}

/// The same exchange without its bodies.
pub fn without_bodies(view: ExchangeView) -> ExchangeView {
    ExchangeView { request_body: None, response_body: None, truncated: false, ..view }
}

/// `body` as text, at most `max` bytes, ending on a character boundary; and whether it was cut.
fn cut(body: &[u8], max: usize) -> (Option<String>, bool) {
    if body.is_empty() {
        return (None, false);
    }
    let text = String::from_utf8_lossy(body);
    if text.len() <= max {
        return (Some(text.into_owned()), false);
    }
    let end = text.floor_char_boundary(max);
    (Some(text.get(..end).unwrap_or_default().to_owned()), true)
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
