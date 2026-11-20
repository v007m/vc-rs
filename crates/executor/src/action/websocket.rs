//! The `websocket` runtime (D376, D379, D386): its handshake is the api's request, and each frame
//! the server sends is one message.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;
use vc_interfaces::{StreamFrame, StreamMode};
use vc_model::{Action, ActionId, Api, Exchange, Failure, Protocol, RenderedRequest, StreamMessage};

use super::protocol::ProtocolRuntime;
use super::request;
use super::stream::{self, Decoder, StreamRuntime};
use crate::Render;

/// The `websocket` type's runtime.
#[derive(Debug)]
pub struct WebsocketRuntime;

impl ProtocolRuntime for WebsocketRuntime {
    fn protocol(&self) -> Protocol {
        Protocol::Websocket
    }

    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        request::rendered(render, api, call)
    }

    fn response(&self, exchange: &Exchange) -> Value {
        stream::received(exchange)
    }

    fn stream(&self) -> Option<&dyn StreamRuntime> {
        Some(self)
    }
}

impl StreamRuntime for WebsocketRuntime {
    fn mode(&self) -> StreamMode {
        StreamMode::WebSocket
    }

    fn decoder(&self) -> Box<dyn Decoder> {
        Box::new(Frames)
    }
}

/// One message per frame; a frame leaves nothing unfinished.
#[derive(Debug)]
struct Frames;

impl Decoder for Frames {
    fn push(&mut self, frame: StreamFrame) -> Vec<StreamMessage> {
        let fields = match frame {
            StreamFrame::Text(text) => {
                vec![("type", Value::from("text")), ("data", stream::data(&text)), ("text", Value::String(text))]
            }
            StreamFrame::Binary(bytes) => {
                vec![("type", Value::from("binary")), ("data", STANDARD.encode(bytes).into())]
            }
            StreamFrame::Close { code, reason } => {
                vec![("type", Value::from("close")), ("code", Value::from(code)), ("reason", Value::String(reason))]
            }
            StreamFrame::Chunk(_) => return Vec::new(),
        };
        vec![stream::message(fields)]
    }
}

#[cfg(test)]
#[path = "websocket_tests.rs"]
mod tests;
