//! The `sse` runtime (D376, D384): its request asks for an event stream, and a pure parser turns
//! the response body into events as the HTML event-stream format reads them.

use std::mem;
use std::time::Duration;

use http::HeaderValue;
use http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use serde_json::Value;
use vc_interfaces::{StreamFrame, StreamMode};
use vc_model::{Action, ActionId, Api, Exchange, Failure, Protocol, RenderedRequest, StreamMessage};

use super::protocol::ProtocolRuntime;
use super::request;
use super::stream::{self, Decoder, Resume, StreamRuntime};
use crate::Render;

const EVENT_STREAM: &str = "text/event-stream";
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// The `sse` type's runtime.
#[derive(Debug)]
pub struct SseRuntime;

impl ProtocolRuntime for SseRuntime {
    fn protocol(&self) -> Protocol {
        Protocol::Sse
    }

    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        let mut request = request::rendered(render, api, call)?;
        for (name, value) in [(ACCEPT, EVENT_STREAM), (CACHE_CONTROL, "no-cache")] {
            if !request::mentions(render, (api, call.1), &name) {
                drop(request.headers.insert(name, HeaderValue::from_static(value)));
            }
        }
        Ok(request)
    }

    fn response(&self, exchange: &Exchange) -> Value {
        stream::received(exchange)
    }

    fn stream(&self) -> Option<&dyn StreamRuntime> {
        Some(self)
    }
}

impl StreamRuntime for SseRuntime {
    fn mode(&self) -> StreamMode {
        StreamMode::Body
    }

    fn decoder(&self) -> Box<dyn Decoder> {
        Box::new(SseDecoder::default())
    }

    fn opened(&self, head: &Exchange) -> Result<(), String> {
        let written = head.response.headers.get(CONTENT_TYPE).and_then(|value| value.to_str().ok());
        let essence = written.and_then(|value| value.split(';').next()).map(str::trim);
        if essence.is_some_and(|essence| essence.eq_ignore_ascii_case(EVENT_STREAM)) {
            return Ok(());
        }
        let what = written.map_or_else(|| "has no content type".to_owned(), |value| format!("is `{value}`"));
        Err(format!("the response {what}, not `{EVENT_STREAM}`"))
    }
}

/// The event-stream parser for one connection: lines end in CR, LF or CRLF, even across chunks.
#[derive(Debug, Default)]
pub struct SseDecoder {
    line: Vec<u8>,
    started: bool,
    after_cr: bool,
    data: Option<String>,
    event: Option<String>,
    retry: Option<u64>,
    last_id: Option<String>,
    last_retry: Option<u64>,
}

impl Decoder for SseDecoder {
    fn push(&mut self, frame: StreamFrame) -> Vec<StreamMessage> {
        let StreamFrame::Chunk(bytes) = frame else { return Vec::new() };
        let mut events = Vec::new();
        for &byte in &bytes {
            if mem::take(&mut self.after_cr) && byte == b'\n' {
                continue;
            }
            match byte {
                b'\r' | b'\n' => {
                    self.after_cr = byte == b'\r';
                    events.extend(self.line_end());
                }
                _ => self.line.push(byte),
            }
        }
        events
    }

    fn resume(&self) -> Resume {
        Resume { last_id: self.last_id.clone(), retry: self.last_retry.map(Duration::from_millis) }
    }
}

impl SseDecoder {
    /// Reads the line just ended: a blank line dispatches the event, `:` starts a comment.
    fn line_end(&mut self) -> Option<StreamMessage> {
        let mut line = mem::take(&mut self.line);
        if !mem::replace(&mut self.started, true) && line.starts_with(BOM) {
            drop(line.drain(..BOM.len()));
        }
        let line = String::from_utf8_lossy(&line);
        if line.is_empty() {
            return self.dispatch();
        }
        if !line.starts_with(':') {
            let (field, value) = line.split_once(':').unwrap_or((&line, ""));
            self.field(field, value.strip_prefix(' ').unwrap_or(value));
        }
        None
    }

    fn field(&mut self, field: &str, value: &str) {
        match field {
            "event" => self.event = Some(value.to_owned()).filter(|event| !event.is_empty()),
            "data" => {
                let data = self.data.get_or_insert_with(String::new);
                data.push_str(value);
                data.push('\n');
            }
            "id" if !value.contains('\0') => self.last_id = Some(value.to_owned()).filter(|id| !id.is_empty()),
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                self.retry = value.parse().ok();
                self.last_retry = self.retry.or(self.last_retry);
            }
            _ => {}
        }
    }

    /// The event the block written so far makes, if it has data; the block is cleared either way.
    fn dispatch(&mut self) -> Option<StreamMessage> {
        let (event, retry) = (self.event.take(), self.retry.take());
        let mut text = self.data.take()?;
        let _: Option<char> = text.pop();
        let mut fields = vec![
            ("event", Value::String(event.unwrap_or_else(|| "message".to_owned()))),
            ("data", stream::data(&text)),
        ];
        fields.extend(self.last_id.clone().map(|id| ("id", Value::String(id))));
        fields.extend(retry.map(|retry| ("retry", Value::from(retry))));
        fields.push(("text", Value::String(text)));
        Some(stream::message(fields))
    }
}

#[cfg(test)]
#[path = "sse_tests.rs"]
mod tests;
