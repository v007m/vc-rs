//! Every api type is registered in every facet (D342): its keys, its compile rules and its
//! runtime, so a new `Protocol` cannot ship half-registered.

use vc_compiler::ProtocolRulesRegistry;
use vc_executor::ProtocolRuntimes;
use vc_model::{PROTOCOL_KEYS, Protocol};

#[test]
fn every_protocol_facet_covers_every_protocol() {
    let keyed: Vec<_> = PROTOCOL_KEYS.iter().map(|(protocol, _)| *protocol).collect();
    let facets = [
        ("PROTOCOL_KEYS", keyed),
        ("ProtocolRulesRegistry::builtin", ProtocolRulesRegistry::builtin().protocols()),
        ("ProtocolRuntimes::standard", ProtocolRuntimes::standard().protocols()),
    ];
    for (facet, covered) in facets {
        assert_eq!(covered, Protocol::ALL, "{facet} must hold exactly one entry per type, in Protocol::ALL order");
    }
}
