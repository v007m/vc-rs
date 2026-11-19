//! The checks each api `type` makes of what it reads of an api ([`ProtocolRules::check`]): a
//! `graphql` api's `body` keys, its `query` and its `operationName` (D343, D344).

use vc_diagnostics::Diagnostics;
use vc_model::FileKind;

use super::ProtocolRulesRegistry;
use super::project;
use super::project::Project;

/// Every api against its type's own checks.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let registry = ProtocolRulesRegistry::builtin();
    let mut diagnostics = Diagnostics::new();
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Api) {
        for api in project::list(source.root, "apis") {
            let Ok(rules) = registry.get(project::protocol(source.root, api)) else { continue };
            diagnostics.extend(rules.check(api));
        }
    }
    diagnostics
}

#[cfg(test)]
#[path = "protocol_checks_tests.rs"]
mod tests;
