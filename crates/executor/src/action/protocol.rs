//! The runtime of each api `type` (D342): how a call to an api of that type builds its request
//! and reads its response. No I/O: the pipeline and its transport send the request.

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use vc_model::{Action, ActionId, Api, Exchange, Failure, Protocol, RenderedRequest};

use super::request;
use crate::Render;

/// How a call to an api of one type is built and read back.
pub trait ProtocolRuntime: fmt::Debug + Send + Sync {
    /// The type this runtime serves.
    fn protocol(&self) -> Protocol;
    /// The request a call to `api` sends.
    fn request(&self, render: &Render<'_>, api: &Api, call: (ActionId, &Action)) -> Result<RenderedRequest, Failure>;
    /// The `response` root a call's checks, saves and later actions read.
    fn response(&self, exchange: &Exchange) -> Value;
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
        Self { runtimes: vec![Arc::new(HttpRuntime)] }
    }

    /// The runtime for `protocol`, if it has one.
    pub(crate) fn get(&self, protocol: Protocol) -> Option<Arc<dyn ProtocolRuntime>> {
        self.runtimes.iter().find(|runtime| runtime.protocol() == protocol).cloned()
    }

    /// The types it has a runtime for, in registration order.
    /// The `response` root an api of `protocol` gives for `exchange`: what its checks, saves and
    /// later actions see. `None` for a type without a runtime.
    #[must_use]
    pub fn response(&self, protocol: Protocol, exchange: &Exchange) -> Option<Value> {
        self.get(protocol).map(|runtime| runtime.response(exchange))
    }

    #[must_use]
    pub fn protocols(&self) -> Vec<Protocol> {
        self.runtimes.iter().map(|runtime| runtime.protocol()).collect()
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
