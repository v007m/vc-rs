//! What a streaming type (D376) adds to its [`ProtocolRuntime`]: how its stream is opened and
//! read, and how its frames become messages (D379). The call path asks the registry entry for it
//! through [`ProtocolRuntime::stream`], so the stream driver is chosen without a `match` on the
//! type.

use std::fmt;
use std::time::Duration;

use serde_json::{Map, Value};
use vc_interfaces::{OutgoingFrame, StreamFrame, StreamMode};
use vc_model::{Exchange, StreamMessage};

use super::request;

/// How one streaming type opens, reads and writes its stream (D421).
pub trait StreamRuntime: fmt::Debug + Send + Sync {
    /// How its stream is read.
    fn mode(&self) -> StreamMode;

    /// A fresh decoder for one connection.
    fn decoder(&self) -> Box<dyn Decoder>;

    /// The frame a rendered `send` value goes out as: a string as its text, anything else as
    /// JSON text.
    fn encode(&self, value: &Value) -> OutgoingFrame {
        match value {
            Value::String(text) => OutgoingFrame::Text(text.clone()),
            other => OutgoingFrame::Text(other.to_string()),
        }
    }

    /// Whether the opening response `head` is one this type reads, else why not (V5131).
    ///
    /// # Errors
    ///
    /// The reason, as V5131 reports it.
    fn opened(&self, head: &Exchange) -> Result<(), String> {
        let _ = head;
        Ok(())
    }
}

/// Turns one connection's frames into messages; it keeps what a frame leaves unfinished.
pub trait Decoder: Send {
    /// The messages `frame` completes, in order.
    fn push(&mut self, frame: StreamFrame) -> Vec<StreamMessage>;

    /// What a reconnect sends and waits for (D385).
    fn resume(&self) -> Resume {
        Resume::default()
    }
}

/// Where a stream left off: the last event id, and the wait the server asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resume {
    /// The last `id` received, sent as `last-event-id`.
    pub last_id: Option<String>,
    /// The last `retry` received.
    pub retry: Option<Duration>,
}

/// The `response` root of a stream call: the opening response as `http` reads it, and its
/// `messages` (D380).
pub fn received(exchange: &Exchange) -> Value {
    let mut response = request::received(exchange);
    let messages = exchange.messages.iter().map(|message| Value::Object(message.fields.clone())).collect();
    if let Some(root) = response.as_object_mut() {
        drop(root.insert("messages".to_owned(), Value::Array(messages)));
    }
    response
}

/// A message's `data`: JSON when `text` parses, else the text (D379).
pub fn data(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned()))
}

/// A message with `fields`.
pub fn message(fields: impl IntoIterator<Item = (&'static str, Value)>) -> StreamMessage {
    let fields: Map<String, Value> = fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect();
    StreamMessage { fields }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
