//! The compile rules of the streaming types (D376): `sse` sends its request as an `http` api
//! does and may reopen its stream once (D385); `websocket` opens with a `GET` and no body.

use vc_model::{BodySpec, Method, Protocol, StreamSettings};
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

    /// Reconnects as the api's `sse.reconnect`, else its file's when the
    /// file is an `sse` one (another file's block is V2213 and read by none), else not.
    fn stream(&self, source: &Source<'_>, api: &Node) -> Option<StreamSettings> {
        let file = Some(source.root).filter(|root| project::protocol(root, root) == Protocol::Sse);
        let mut blocks = [Some(api), file].into_iter().flatten().filter_map(|node| node.get("sse"));
        let written = blocks.find(|sse| project::text(sse, "reconnect").is_some());
        let reconnect = written.is_some_and(|sse| read::flag(shapes::sse(), sse, "reconnect"));
        Some(StreamSettings { reconnect })
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

    /// Never reconnects.
    fn stream(&self, _: &Source<'_>, _: &Node) -> Option<StreamSettings> {
        Some(StreamSettings { reconnect: false })
    }
}

#[cfg(test)]
#[path = "stream_tests.rs"]
mod tests;
