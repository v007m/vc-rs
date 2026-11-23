//! V4014: an auth's `secretAccessKey` or `sessionToken` written as text in the config, so the
//! credential sits in git with it (D440, D446). `${secret.NAME}` reads it from the
//! environment or `.env` instead. The message names the key, never its value, and compiling
//! masks the value in every diagnostic, so no excerpt prints it either.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Entry;

use super::tls_literals::written_secret;
use super::{Lint, LintCx};
use crate::link;

/// Reports each literal `secretAccessKey` and `sessionToken` in the config's auths.
pub struct Sigv4Secrets;

impl Lint for Sigv4Secrets {
    fn code(&self) -> Code {
        Code::V4014
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(config) = cx.project.config() else { return Vec::new() };
        link::sigv4_secrets(config.root).into_iter().map(|entry| self.literal(entry)).collect()
    }
}

impl Sigv4Secrets {
    fn literal(&self, entry: &Entry) -> Diagnostic {
        let what = if entry.key == "sessionToken" { "a session token" } else { "a secret access key" };
        written_secret(self.code(), entry, what)
    }
}
