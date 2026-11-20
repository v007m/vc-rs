//! The runtime of each api `type` (D342): how a call to an api of that type builds its request
//! and reads its response. No I/O: the pipeline and its transport send the request.

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use vc_model::{Action, ActionId, Api, Exchange, Failure, Protocol, RenderedRequest};

use super::graphql::GraphqlRuntime;
use super::request;
use super::sse::SseRuntime;
use super::stream::StreamRuntime;
use super::websocket::WebsocketRuntime;
use crate::Render;

/// How a call to an api of one type is built and read back.
pub trait ProtocolRuntime: fmt::Debug + Send + Sync {
    /// The type this runtime serves.
    fn protocol(&self) -> Protocol;
    /// The request a call to `api` sends.
    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure>;
    /// The `response` root a call's checks, saves and later actions read.
    fn response(&self, exchange: &Exchange) -> Value;
    /// The errors `response` (the root [`Self::response`] gave) carries, normalised, which hold
    /// back an api's default `save` and its `cleanup` (D345); none for a type without them.
    ///
    /// # Errors
    ///
    /// A failure when `response` is not one this type answers with.
    fn errors(&self, response: &Value) -> Result<Vec<Value>, Failure> {
        let _ = response;
        Ok(Vec::new())
    }
    /// How this type opens and reads a stream (D421); `None` for a type whose call is one
    /// request and one response.
    fn stream(&self) -> Option<&dyn StreamRuntime> {
        None
    }
}

/// `http`: the api's method, URL, headers and body as written; the response as received.
#[derive(Debug)]
pub struct HttpRuntime;

impl ProtocolRuntime for HttpRuntime {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }

    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure> {
        request::rendered(render, api, call)
    }

    fn response(&self, exchange: &Exchange) -> Value {
        request::received(exchange)
    }
}

/// The runtime of every api type, one entry per [`Protocol`].
#[derive(Clone, Debug)]
pub struct ProtocolRuntimes {
    runtimes: Vec<Arc<dyn ProtocolRuntime>>,
}

impl ProtocolRuntimes {
    /// The runtime of every built-in type.
    #[must_use]
    pub fn standard() -> Self {
        let runtimes: Vec<Arc<dyn ProtocolRuntime>> =
            vec![Arc::new(HttpRuntime), Arc::new(GraphqlRuntime), Arc::new(SseRuntime), Arc::new(WebsocketRuntime)];
        Self { runtimes }
    }

    /// The runtime for `protocol`, if it has one.
    pub(crate) fn get(&self, protocol: Protocol) -> Option<Arc<dyn ProtocolRuntime>> {
        self.runtimes.iter().find(|runtime| runtime.protocol() == protocol).cloned()
    }

    /// The `response` root an api of `protocol` gives for `exchange`: what its checks, saves and
    /// later actions see. `None` for a type without a runtime.
    #[must_use]
    pub fn response(&self, protocol: Protocol, exchange: &Exchange) -> Option<Value> {
        self.get(protocol).map(|runtime| runtime.response(exchange))
    }

    /// The registry with `runtime` serving its type in place of the one there.
    #[cfg(test)]
    #[must_use]
    pub fn with(mut self, runtime: Arc<dyn ProtocolRuntime>) -> Self {
        self.runtimes.retain(|held| held.protocol() != runtime.protocol());
        self.runtimes.push(runtime);
        self
    }

    /// The types it has a runtime for, in registration order.
    #[must_use]
    pub fn protocols(&self) -> Vec<Protocol> {
        self.runtimes.iter().map(|runtime| runtime.protocol()).collect()
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
