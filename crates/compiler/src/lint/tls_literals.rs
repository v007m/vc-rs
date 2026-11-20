//! V4013: a client key written as inline PEM, or a `passphrase` written as text, in
//! `http.tls.clients`, so the secret sits in git with the config (D353). `${secret.NAME}` reads
//! it from `.env` instead. The message names the key, never its value, and compiling masks the
//! value in every diagnostic, so no excerpt prints it either.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::Entry;

use super::{Lint, LintCx};
use crate::link;

/// Reports each literal `key` PEM and `passphrase` in the config's `http.tls.clients`.
pub struct TlsLiterals;

impl Lint for TlsLiterals {
    fn code(&self) -> Code {
        Code::V4013
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let Some(config) = cx.project.config() else { return Vec::new() };
        link::tls_literals(config.root).into_iter().map(|entry| self.literal(entry)).collect()
    }
}

impl TlsLiterals {
    fn literal(&self, entry: &Entry) -> Diagnostic {
        let what = if entry.key == "key" { "a private key" } else { "a passphrase" };
        written_secret(self.code(), entry, what)
    }
}

/// `code` at `entry`'s key: the secret `what` is written in the config. It names the key, never
/// the value.
pub(super) fn written_secret(code: Code, entry: &Entry, what: &str) -> Diagnostic {
    let name = &entry.key;
    Diagnostic::new(code, entry.key_span, format!("`{name}` is {what} written in the config"))
        .label("its value is kept in git with the file")
        .help(format!("read it from `.env`: `{name}: ${{secret.NAME}}`, with `NAME=` declared in `.env.schema`"))
}
