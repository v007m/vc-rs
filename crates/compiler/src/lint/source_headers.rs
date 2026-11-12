//! V4009: a credential header in an api file's `source.headers` written as a literal value, so
//! the token sits in git with the file. `${secret.NAME}` reads it from `.env` instead (D314).
//! The message names the header, never its value: the key is underlined and the value is
//! masked, so the excerpt never prints the token either.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{FileKind, credential};
use vc_parser::{Entry, Node, ScalarKind};

use super::{Lint, LintCx};

/// Reports each credential `source.headers` entry whose value holds no `${…}`.
pub struct SourceHeaders;

impl Lint for SourceHeaders {
    fn code(&self) -> Code {
        Code::V4009
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let apis = cx.project.sources.iter().filter(|source| source.kind == FileKind::Api);
        let headers = apis.filter_map(|source| source.root.get("source")?.get("headers")).flat_map(Node::entries);
        headers.filter(|entry| literal(entry)).map(|entry| self.literal(entry)).collect()
    }
}

impl SourceHeaders {
    fn literal(&self, entry: &Entry) -> Diagnostic {
        let name = &entry.key;
        let message = format!("`source.headers.{name}` is a credential written as a literal value");
        let help =
            format!("read it from `.env`: `{name}: Bearer ${{secret.NAME}}`, with `NAME=` declared in `.env.schema`");
        Diagnostic::new(self.code(), entry.key_span, message)
            .label("its value is kept in git with the file")
            .masked(entry.value.span)
            .help(help)
    }
}

/// Whether `entry` names a credential and its value is text with no `${…}` in it.
fn literal(entry: &Entry) -> bool {
    let Some(scalar) = entry.value.as_scalar().filter(|scalar| scalar.kind != ScalarKind::Null) else {
        return false;
    };
    let text = scalar.text.replace("$${", "");
    credential(&entry.key) && !text.trim().is_empty() && !text.contains("${")
}
