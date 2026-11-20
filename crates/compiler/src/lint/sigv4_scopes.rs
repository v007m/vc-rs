//! V2316: an `awsSigV4` auth's `region` or `service` written as text that is not a credential
//! scope name: lowercase letters, digits and `-` (D440). Every signature names that scope, so a
//! run with one would be refused on every request. Lowercasing it is the certain fix when that
//! is enough. A value with a `${…}` in it is left to the run.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_parser::{Entry, write_text};

use super::{Lint, LintCx};
use crate::link;
use crate::naming::is_lower_word;

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

/// Whether `text` is a credential scope part: a lowercase ASCII letter, then letters, digits and
/// `-`, as every AWS region code and signing name is.
fn scope_name(text: &str) -> bool {
    is_lower_word(text, true)
}

#[cfg(test)]
mod tests {
    use super::scope_name;

    #[test]
    fn a_scope_name_is_lowercase_letters_digits_and_dashes() {
        for good in ["us-east-1", "execute-api", "s3", "aoss"] {
            assert!(scope_name(good), "{good}");
        }
        for bad in ["", "US-East-1", "execute api", "execute_api", "eu-west-1\n", "ü", "s3.", "1a"] {
            assert!(!scope_name(bad), "{bad:?}");
        }
    }
}
