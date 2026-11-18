//! The compile rules of each api `type` (D342): the parts of an api that its type decides.

use vc_model::{BodySpec, Method, ModelError, Protocol, Template};
use vc_parser::Node;

use super::graphql::GraphqlRules;
use super::read;
use crate::link::project::Source;

/// How an api of one type reads into the program.
pub trait ProtocolRules {
    /// The type these rules compile.
    fn protocol(&self) -> Protocol;
    /// The method the api sends.
    fn method(&self, api: &Node) -> Method;
    /// The body the api sends.
    fn body(&self, source: &Source<'_>, api: &Node) -> BodySpec;
    /// The path the api sends to; `None` when it has none (V1005 reports it).
    fn path(&self, source: &Source<'_>, api: &Node) -> Option<Template>;
}

/// `http`: the api's own `method`, else GET, and its own `body` and `path`.
struct HttpRules;

impl ProtocolRules for HttpRules {
    fn protocol(&self) -> Protocol {
        Protocol::Http
    }

    fn method(&self, api: &Node) -> Method {
        read::method(api)
    }

    fn body(&self, source: &Source<'_>, api: &Node) -> BodySpec {
        api.get("body").and_then(|body| read::body(source, body)).unwrap_or_default()
    }

    fn path(&self, source: &Source<'_>, api: &Node) -> Option<Template> {
        api.get("path").and_then(|path| read::template(source, path))
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
        let rules: Vec<Box<dyn ProtocolRules>> = vec![Box::new(HttpRules), Box::new(GraphqlRules)];
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
