//! The keys each built-in auth type needs (syntax reference "Authentication"): a definition
//! that leaves one out is V1005 when checked, not V5114 on its first call.

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_parser::Node;

use super::project;
use super::project::Project;

/// Each auth type, with the keys it cannot work without.
const NEEDS: [(&str, &[&str]); 9] = [
    ("basic", &["username", "password"]),
    ("bearer", &["token"]),
    ("apiKey", &["name", "value"]),
    ("oauth2ClientCredentials", &["tokenUrl", "clientId"]),
    ("oauth2Password", &["tokenUrl", "clientId", "username", "password"]),
    ("oauth2AuthorizationCode", &["authorizeUrl", "tokenUrl", "clientId"]),
    ("oauth2Refresh", &["tokenUrl", "clientId", "refreshToken"]),
    ("flow", &["steps"]),
    ("plugin", &["plugin"]),
];

/// Every config `auth` entry against the keys its `type` needs.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    let Some(config) = project.config() else { return diagnostics };
    for entry in project::list(config.root, "auth") {
        let Some(kind) = entry.get("type") else { continue };
        let Some((name, needs)) = NEEDS.iter().find(|(name, _)| kind.as_str() == Some(*name)) else { continue };
        for key in needs.iter().filter(|key| entry.get(key).is_none()) {
            diagnostics.push(missing(kind, (name, key)));
        }
    }
    diagnostics
}

/// V1005 at the entry's `type`, naming the key it needs.
fn missing(kind: &Node, (name, key): (&str, &str)) -> Diagnostic {
    let message = format!("a `{name}` auth needs `{key}`");
    Diagnostic::new(Code::V1005, kind.span, message)
        .label(format!("`{key}` is missing"))
        .help(format!("add `{key}:`; a credential is written as a template such as `${{secret.PARTNER_TOKEN}}`"))
}
