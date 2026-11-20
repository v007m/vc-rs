//! The runtime of the streaming types (D376) until their calls run (#92, #93): a call of an
//! `sse` or `websocket` api fails with a plain message before anything is sent.

use serde_json::Value;
use vc_model::{Action, ActionId, Api, Exchange, Failure, Protocol, RenderedRequest};

use super::protocol::ProtocolRuntime;
use super::request;
use crate::Render;

/// A streaming type whose calls do not run yet.
#[derive(Debug)]
pub struct PendingStream(pub Protocol);

impl ProtocolRuntime for PendingStream {
    fn protocol(&self) -> Protocol {
        self.0
    }

    fn request(&self, _: &Render<'_>, api: &Api, _: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        let kind = api.protocol.key();
        Err(Failure::new(format!("a `{kind}` api cannot be called yet: stream calls are not run (#92, #93)")))
    }

    fn response(&self, exchange: &Exchange) -> Value {
        request::received(exchange)
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
