//! V4013: a client key written as inline PEM, or a `passphrase` written as text, in
//! `http.tls.clients`, so the secret sits in git with the config (D353). `${secret.NAME}` reads
//! it from `.env` instead. The message names the key, never its value: the value is masked, so
//! the excerpt never prints it either.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Entry, ScalarKind};

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
        let Some(tls) = config.root.get("http").and_then(|http| http.get("tls")) else { return Vec::new() };
        let clients = link::list(tls, "clients");
        let entries = clients.iter().flat_map(|client| ["key", "passphrase"].map(|key| client.entry(key)));
        entries.flatten().filter(|entry| literal(entry)).map(|entry| self.literal(entry)).collect()
    }
}

impl TlsLiterals {
    fn literal(&self, entry: &Entry) -> Diagnostic {
        let name = &entry.key;
        let what = if name == "key" { "a private key" } else { "a passphrase" };
        Diagnostic::new(self.code(), entry.key_span, format!("`{name}` is {what} written in the config"))
            .label("its value is kept in git with the file")
            .masked(entry.value.span)
            .help(format!("read it from `.env`: `{name}: ${{secret.NAME}}`, with `NAME=` declared in `.env.schema`"))
    }
}

/// Whether `entry` holds text with no `${…}`: for `key`, inline PEM; for `passphrase`, any text.
fn literal(entry: &Entry) -> bool {
    let Some(scalar) = entry.value.as_scalar().filter(|scalar| scalar.kind != ScalarKind::Null) else {
        return false;
    };
    let text = scalar.text.replace("$${", "");
    let secret = if entry.key == "key" { text.trim_start().starts_with("-----BEGIN") } else { !text.is_empty() };
    secret && !text.contains("${")
}
