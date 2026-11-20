//! The keys each built-in auth type needs ([`vc_model::AUTH_NEEDS`] and
//! [`vc_model::AUTH_OWN_NEEDS`]): a definition that leaves one out is V1005 when checked, not
//! V5114 on its first call. `mtls` needs `clientCert` (D350).

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_model::{auth_needs, auth_own_needs};
use vc_parser::Node;

use super::project;
use super::project::Project;

/// The help for each needed key that is not a credential: the [`vc_model::AUTH_OWN_NEEDS`] and
/// an `awsSigV4` auth's scope.
const PLAIN_HELP: [(&str, &str); 3] = [
    ("clientCert", "add `clientCert:` with the ID of an `http.tls.clients` entry"),
    ("region", "add `region:` with the signing region, such as `us-east-1`"),
    ("service", "add `service:` with the signing name, such as `execute-api` or `s3`"),
];

/// The help for a credential the provider reads.
const CREDENTIAL_HELP: &str = "a credential is written as a template such as `${secret.PARTNER_TOKEN}`";

/// Every config `auth` entry against the keys its `type` needs.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let Some(config) = project.config() else { return diagnostics };
    for entry in project::list(config.root, "auth") {
        let Some(kind) = entry.get("type") else { continue };
        let Some(name) = kind.as_str() else { continue };
        let settings = auth_needs(name).unwrap_or_default().iter().map(|key| (*key, plain_help(key)));
        let own = auth_own_needs(name).iter().map(|key| (*key, plain_help(key)));
        for (key, help) in settings.chain(own).filter(|(key, _)| entry.get(key).is_none()) {
            diagnostics.push(missing(kind, (name, key), help));
        }
    }
    diagnostics
}

/// The help for `key` when it is not a credential.
fn plain_help(key: &str) -> Option<&'static str> {
    PLAIN_HELP.iter().find(|(own, _)| *own == key).map(|(_, help)| *help)
}

/// V1005 at the entry's `type`, naming the key it needs.
fn missing(kind: &Node, (name, key): (&str, &str), help: Option<&str>) -> Diagnostic {
    let message = format!("a `{name}` auth needs `{key}`");
    let help = help.map_or_else(|| format!("add `{key}:`; {CREDENTIAL_HELP}"), str::to_owned);
    Diagnostic::new(Code::V1005, kind.span, message).label(format!("`{key}` is missing")).help(help)
}
