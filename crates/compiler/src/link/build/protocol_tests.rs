use vc_model::{ModelError, Protocol};

use super::{ProtocolRules, ProtocolRulesRegistry};

#[test]
fn the_builtin_compile_rules_cover_every_protocol() {
    let rules = ProtocolRulesRegistry::builtin();
    assert_eq!(rules.protocols(), Protocol::ALL);
    for protocol in Protocol::ALL {
        assert_eq!(rules.get(protocol).map(ProtocolRules::protocol).ok(), Some(protocol));
    }
}

#[test]
fn a_type_without_compile_rules_is_an_error_not_a_missing_api() {
    let rules = ProtocolRulesRegistry { rules: Vec::new() };
    let missing = rules.get(Protocol::Http).map(ProtocolRules::protocol);
    assert_eq!(missing, Err(ModelError::NoProtocolRules(Protocol::Http)));
    assert_eq!(ModelError::NoProtocolRules(Protocol::Http).to_string(), "no compile rules for an `http` api");
}
