//! The checks each api `type` makes of what it reads of an api ([`ProtocolRules::check`]): a
//! `graphql` api's `body` keys, its `query` and its `operationName` (D343, D344, D356); its file's
//! `graphql.schema` (D347); V1014 for each GraphQL document that does not parse; and V1015 for each
//! schema file that is not a schema.

use vc_diagnostics::Diagnostics;
use vc_model::FileKind;

use super::ProtocolRulesRegistry;
use super::project;
use super::project::Project;
use crate::graphql::Documents;
use crate::graphql_schema;

/// Every api against its type's own checks, and every document and schema of `documents` that
/// does not read.
pub fn check(project: &Project<'_>, documents: &Documents<'_>) -> Diagnostics {
    let registry = ProtocolRulesRegistry::builtin();
    let mut diagnostics: Diagnostics = documents.problems().iter().cloned().collect();
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Api) {
        for api in project::list(source.root, "apis") {
            let Ok(rules) = registry.get(project::protocol(source.root, api)) else { continue };
            diagnostics.extend(rules.check(source, api));
        }
        diagnostics.extend(graphql_schema::check(source));
    }
    diagnostics.extend(graphql_schema::problems(project, documents));
    diagnostics
}

#[cfg(test)]
#[path = "protocol_checks_tests.rs"]
mod tests;
