//! V2316: an `awsSigV4` auth's `region` or `service` written as text that is not a credential
//! scope name: lowercase letters, digits and `-` (D440). Every signature names that scope, so a
//! run with one would be refused on every request. Lowercasing it is the certain fix when that
//! is enough. A value with a `${…}` in it is checked when it renders.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_parser::{Entry, write_text};

use super::{Lint, LintCx};
use crate::link;

/// Reports each literal `region` and `service` in the config's `awsSigV4` auths that AWS would
/// not take.
pub struct Sigv4Scopes;

impl Lint for Sigv4Scopes {
    fn code(&self) -> Code {
        Code::V2316
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(config) = cx.project.config() else { return Vec::new() };
        let scopes = link::sigv4_scopes(config.root).into_iter().filter(|(_, text)| !scope_name(text));
        scopes.map(|(entry, text)| self.bad(entry, text)).collect()
    }
}

impl Sigv4Scopes {
    fn bad(&self, entry: &Entry, text: &str) -> Diagnostic {
        let (what, like) = if entry.key == "region" {
            ("an AWS region", "`us-east-1`")
        } else {
            ("an AWS signing name", "`execute-api` or `s3`")
        };
        let message =
            if text.is_empty() { format!("`{}` is empty", entry.key) } else { format!("`{text}` is not {what}") };
        let diagnostic = Diagnostic::new(self.code(), entry.value.span, message)
            .label("a scope name is lowercase letters, digits and `-`")
            .help(format!("write {what}, such as {like}"));
        let lower = text.to_ascii_lowercase();
        if !scope_name(&lower) {
            return diagnostic;
        }
        let edit = TextEdit::replace(entry.value.span, write_text(&lower, false));
        diagnostic.fix(Fix::certain(format!("Write {lower}"), vec![edit]))
    }
}

/// Whether `text` is a credential scope part: lowercase ASCII letters, digits and `-`, not empty.
fn scope_name(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::scope_name;

    #[test]
    fn a_scope_name_is_lowercase_letters_digits_and_dashes() {
        for good in ["us-east-1", "execute-api", "s3", "aoss", "1"] {
            assert!(scope_name(good), "{good}");
        }
        for bad in ["", "US-East-1", "execute api", "execute_api", "eu-west-1\n", "ü", "s3."] {
            assert!(!scope_name(bad), "{bad:?}");
        }
    }
}
