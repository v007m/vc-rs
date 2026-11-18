use vc_model::Protocol;

use super::{ProtocolRules, ProtocolRulesRegistry};

#[test]
fn the_builtin_compile_rules_cover_every_protocol() {
    let rules = ProtocolRulesRegistry::builtin();
    assert_eq!(rules.protocols(), Protocol::ALL);
    for protocol in Protocol::ALL {
        assert_eq!(rules.get(protocol).map(ProtocolRules::protocol), Some(protocol));
    }
}
