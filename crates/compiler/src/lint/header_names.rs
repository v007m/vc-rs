//! V2310: a `headers` key that is no HTTP header name, so no request can send it: a space, a
//! colon or another character outside RFC 9110's token, or a `${…}` template, which a header
//! name never renders.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::FileKind;
use vc_parser::{Entry, Node};

use super::{Lint, LintCx};
use crate::link;
use crate::link::Source;

/// The characters a header name may hold besides letters and digits (RFC 9110 `tchar`).
const MARKS: &str = "!#$%&'*+-.^_`|~";

/// Reports every request and check header whose name is no token.
pub struct HeaderNames;

impl Lint for HeaderNames {
    fn code(&self) -> Code {
        Code::V2310
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let holders = cx.project.sources.iter().flat_map(holders);
        let headers = holders.filter_map(|holder| holder.get("headers")).flat_map(Node::entries);
        headers.filter(|entry| !is_token(&entry.key)).map(|entry| self.bad(entry)).collect()
    }
}

impl HeaderNames {
    fn bad(&self, entry: &Entry) -> Diagnostic {
        let name = &entry.key;
        let (label, help) = if name.contains("${") {
            ("a header name is never rendered", "write the name itself; put the template in the value".to_owned())
        } else {
            ("not a header name", format!("a header name has letters, digits and {MARKS} only, such as `X-Request-Id`"))
        };
        let message = format!("`{name}` is not an HTTP header name, so no request can send it");
        Diagnostic::new(self.code(), entry.key_span, message).label(label).help(help)
    }
}

/// The maps of `source` that may hold `headers`: requests, auth flow steps and what they add,
/// and the checks on responses.
fn holders<'n>(source: &Source<'n>) -> Vec<&'n Node> {
    let root = source.root;
    let mut found: Vec<&Node> = match source.kind {
        FileKind::Config => {
            let auths = link::list(root, "auth");
            let steps = auths.iter().flat_map(|auth| link::list(auth, "steps"));
            let applied = auths.iter().filter_map(|auth| auth.get("apply"));
            root.get("defaults").into_iter().chain(steps).chain(applied).chain(link::actions(source)).collect()
        }
        FileKind::Api => std::iter::once(root).chain(root.get("source")).chain(link::list(root, "apis")).collect(),
        FileKind::Test => link::actions(source),
        FileKind::Load => Vec::new(),
    };
    let checks: Vec<&Node> = found.iter().filter_map(|holder| holder.get("expect")).collect();
    found.extend(checks);
    found
}

/// Whether `name` is an RFC 9110 token: one or more letters, digits and [`MARKS`].
fn is_token(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || MARKS.contains(c))
}

#[cfg(test)]
mod tests {
    use super::is_token;

    #[test]
    fn a_header_name_is_a_token() {
        assert!(is_token("X-Request-Id") && is_token("x_trace.v2") && is_token("Accept"));
        assert!(!is_token("x bad") && !is_token("${var.h}") && !is_token("a:b") && !is_token(""));
        assert!(!is_token("Ünicode"), "a header name is ASCII");
    }
}
