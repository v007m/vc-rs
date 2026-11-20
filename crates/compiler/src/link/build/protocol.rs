//! The compile rules of each api `type` (D342): the parts of an api that its type decides.

use vc_diagnostics::Diagnostic;
use vc_model::{BodySpec, Method, ModelError, Protocol, StreamSettings, Template};
use vc_parser::Node;

use super::graphql::GraphqlRules;
use super::read;
use super::stream::{SseRules, WebsocketRules};
use crate::link::data::Data;
use crate::link::project::Source;

/// How an api of one type reads into the program. The request methods default to `http`'s:
/// the api's own `method` (else GET), `body` and `path`, its whole `body` a template.
pub trait ProtocolRules {
    /// The type these rules compile.
    fn protocol(&self) -> Protocol;
    /// The method the api sends.
    fn method(&self, api: &Node) -> Method {
        read::method(api)
    }
    /// The body the api sends; `data` holds the files the compile was given.
    fn body(&self, (source, _): (&Source<'_>, &Data<'_>), api: &Node) -> BodySpec {
        api.get("body").and_then(|body| read::body(source, body)).unwrap_or_default()
    }
    /// The path the api sends to; `None` when it has none (V1005 reports it).
    fn path(&self, source: &Source<'_>, api: &Node) -> Option<Template> {
        api.get("path").and_then(|path| read::template(source, path))
    }
    /// The keys of the api's `body` that are templates; `None` when all of it is.
    fn templated_body_keys(&self) -> Option<&'static [&'static str]> {
        None
    }
    /// The problems in what the type reads of the api, beyond its keys and its shape.
    fn check(&self, _source: &Source<'_>, _api: &Node) -> Vec<Diagnostic> {
        Vec::new()
    }
    /// How the api's stream opens and whether it reopens (D376, D385); `None` for a type that
    /// does not stream.
    fn stream(&self, _source: &Source<'_>, _api: &Node) -> Option<StreamSettings> {
        None
    }
}

/// `http`: the request as the defaults read it.
struct HttpRules;

impl ProtocolRules for HttpRules {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }
}

/// The compile rules of every api type, one entry per [`Protocol`].
pub struct ProtocolRulesRegistry {
    rules: Vec<Box<dyn ProtocolRules>>,
}

impl ProtocolRulesRegistry {
    /// The rules of every built-in type.
    #[must_use]
    pub fn builtin() -> Self {
        let rules: Vec<Box<dyn ProtocolRules>> =
            vec![Box::new(HttpRules), Box::new(GraphqlRules), Box::new(SseRules), Box::new(WebsocketRules)];
        Self { rules }
    }

    /// The rules for `protocol`; a type without them fails the build (`NoProtocolRules`).
    pub(crate) fn get(&self, protocol: Protocol) -> Result<&dyn ProtocolRules, ModelError> {
        let found = self.rules.iter().find(|rules| rules.protocol() == protocol);
        found.map(AsRef::as_ref).ok_or(ModelError::NoProtocolRules(protocol))
    }

    /// The types it has rules for, in registration order.
    #[must_use]
    pub fn protocols(&self) -> Vec<Protocol> {
        self.rules.iter().map(|rules| rules.protocol()).collect()
    }
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
