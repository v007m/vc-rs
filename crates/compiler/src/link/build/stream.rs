//! The compile rules of the streaming types (D376): `sse` sends its request as an `http` api
//! does and may reopen its stream once (D385); `websocket` opens with a `GET` and no body.

use vc_model::{BodySpec, Method, Protocol};
use vc_parser::Node;

use super::protocol::ProtocolRules;
use super::read;
use crate::link::data::Data;
use crate::link::project;
use crate::link::project::Source;
use crate::shapes;

/// `sse`: the api's own `method` (else GET), `body` and `path`, as for `http`.
pub struct SseRules;

impl ProtocolRules for SseRules {
    fn protocol(&self) -> Protocol {
        Protocol::Sse
    }

    /// The api's `sse.reconnect`, else its file's, else `false`.
    fn reconnect(&self, source: &Source<'_>, api: &Node) -> bool {
        let mut blocks = [api, source.root].into_iter().filter_map(|node| node.get("sse"));
        let written = blocks.find(|sse| project::text(sse, "reconnect").is_some());
        written.is_some_and(|sse| read::flag(shapes::sse(), sse, "reconnect"))
    }
}

/// `websocket`: the handshake is always a `GET` with no body, to the api's own `path`.
pub struct WebsocketRules;

impl ProtocolRules for WebsocketRules {
    fn protocol(&self) -> Protocol {
        Protocol::Websocket
    }

    fn method(&self, _: &Node) -> Method {
        Method::Get
    }

    fn body(&self, _: (&Source<'_>, &Data<'_>), _: &Node) -> BodySpec {
        BodySpec::None
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
