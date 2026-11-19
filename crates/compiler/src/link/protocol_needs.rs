//! The keys each api `type` cannot do without ([`vc_model::PROTOCOL_NEEDS`]): an `http` api
//! needs `path`, a `graphql` api `body.query` (D343). A missing one is V1005.

use vc_diagnostics::{Diagnostic, Diagnostics};
use vc_model::{FileKind, PROTOCOL_NEEDS, Protocol};
use vc_parser::Node;

use super::project;
use super::project::Project;
use crate::{shape_check, shapes};

/// Every api against the keys its type needs.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for source in project.sources.iter().filter(|source| source.kind == FileKind::Api) {
        for api in project::list(source.root, "apis") {
            let protocol = project::protocol(source.root, api);
            let needs = PROTOCOL_NEEDS.iter().filter(|(needing, _)| *needing == protocol);
            for key in needs.flat_map(|(_, keys)| keys.iter()).filter(|key| !written(api, key)) {
                diagnostics.push(missing(api, (protocol, key)));
            }
        }
    }
    diagnostics
}

/// Whether the dotted `path` is set under `node`.
fn written(node: &Node, path: &str) -> bool {
    path.split('.').try_fold(node, |node, key| node.get(key)).is_some()
}

/// V1005 at the api's first key, as the shape check reports a missing key: an `http` api is
/// the plain one, so it is named as the shape check names it.
fn missing(api: &Node, (protocol, key): (Protocol, &str)) -> Diagnostic {
    let subject = (protocol != Protocol::default()).then(|| format!("a `{}` api", protocol.key()));
    shape_check::missing_key(api, shapes::api(), (key, subject.as_deref()))
}

#[cfg(test)]
#[path = "protocol_needs_tests.rs"]
mod tests;
