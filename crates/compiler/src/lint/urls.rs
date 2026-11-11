//! V4005: a `baseUrl` and `path` that do not join with one `/`. They are joined as plain text,
//! so `baseUrl` is written without a trailing `/` and every `path` with a leading one.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::FileKind;
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link;

/// Reports a `baseUrl` that ends in `/` and a `path` that does not start with one.
pub struct ApiUrls;

impl Lint for ApiUrls {
    fn code(&self) -> Code {
        Code::V4005
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        for source in cx.project.sources.iter().filter(|source| source.kind == FileKind::Api) {
            found.extend(source.root.get("baseUrl").and_then(|base| self.base(base)));
            let paths = link::list(source.root, "apis").iter().filter_map(|api| api.get("path"));
            found.extend(paths.filter_map(|path| self.path(path)));
        }
        found
    }
}

impl ApiUrls {
    /// `baseUrl: ${var.usersUrl}/`, which a path's leading `/` doubles.
    fn base(&self, node: &Node) -> Option<Diagnostic> {
        let text = node.as_str()?.strip_suffix('/')?;
        let diagnostic = Diagnostic::new(self.code(), node.span, "`baseUrl` ends in `/`")
            .label("doubled by each path's leading `/`")
            .help(format!("write it without the trailing `/`: `baseUrl: {text}`"));
        Some(fixed(node, diagnostic, ("Remove the trailing `/`", text.to_owned())))
    }

    /// `path: users`, which runs into the host; a full URL or one starting with a template is
    /// left alone.
    fn path(&self, node: &Node) -> Option<Diagnostic> {
        let text = node.as_str()?;
        if text.starts_with('/') || text.starts_with("${") || text.contains("://") {
            return None;
        }
        let diagnostic = Diagnostic::new(self.code(), node.span, "`path` does not start with `/`")
            .label("runs into the base URL's host")
            .help(format!("start it with `/`: `path: /{text}`"));
        Some(fixed(node, diagnostic, ("Add the leading `/`", format!("/{text}"))))
    }
}

/// `diagnostic` with the certain fix that writes `new_text` over `node`, when `node` is written
/// as is, without quotes.
fn fixed(node: &Node, diagnostic: Diagnostic, (title, new_text): (&str, String)) -> Diagnostic {
    let plain = node.as_str().is_some_and(|text| node.span.byte_range().len() == text.len());
    if !plain {
        return diagnostic;
    }
    diagnostic.fix(Fix::certain(title, vec![TextEdit::replace(node.span, new_text)]))
}
