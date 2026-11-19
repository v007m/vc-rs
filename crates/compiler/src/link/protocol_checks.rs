//! The checks each api `type` makes of what it reads of an api ([`ProtocolRules::check`]): a
//! `graphql` api's `body` keys, its `query` and its `operationName` (D343, D344, D356); and V1014
//! for each GraphQL document that does not parse.

use vc_diagnostics::Diagnostics;
use vc_model::FileKind;

use super::ProtocolRulesRegistry;
use super::project;
use super::project::Project;
use crate::graphql::Documents;

/// Every api against its type's own checks, and every document of `documents` that does not parse.
pub fn check(project: &Project<'_>, documents: &Documents<'_>) -> Diagnostics {
    let registry = ProtocolRulesRegistry::builtin();
    let mut diagnostics: Diagnostics = documents.problems().iter().cloned().collect();
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Api) {
        for api in project::list(source.root, "apis") {
            let Ok(rules) = registry.get(project::protocol(source.root, api)) else { continue };
            diagnostics.extend(rules.check(source, api));
        }
    }
    diagnostics
}

#[cfg(test)]
#[path = "protocol_checks_tests.rs"]
mod tests;
