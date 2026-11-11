//! V4006: a `{name}` placeholder in an api's `path`, the way an `OpenAPI` spec writes one. A path is a
//! template, so `{jobId}` is sent as the text `{jobId}`; the var is `${var.jobId}`.

use vc_diagnostics::{Code, Diagnostic, Fix, TextEdit};
use vc_model::FileKind;
use vc_parser::Node;

use super::{Lint, LintCx};
use crate::link;

/// Reports an api `path` that holds a `{name}` placeholder.
pub struct PathPlaceholders;

impl Lint for PathPlaceholders {
    fn code(&self) -> Code {
        Code::V4006
    }

    fn check(&self, cx: &LintCx<'_, '_>) -> Vec<Diagnostic> {
        let apis = cx.project.sources.iter().filter(|source| source.kind == FileKind::Api);
        let paths = apis.flat_map(|source| link::list(source.root, "apis")).filter_map(|api| api.get("path"));
        paths.filter_map(|path| self.placeholder(path)).collect()
    }
}

impl PathPlaceholders {
    /// `path: /poll/{jobId}`, with the suggested fix that writes each placeholder as a var.
    fn placeholder(&self, node: &Node) -> Option<Diagnostic> {
        let text = node.as_str()?;
        let found = placeholders(text);
        let (_, first) = found.first()?;
        let message = format!("`{{{first}}}` in a `path` is sent as is; it is not a var");
        let diagnostic = Diagnostic::new(self.code(), node.span, message)
            .label("a placeholder, not a template")
            .help(format!("write each one as a var: `${{var.{first}}}`, and set it in the test's `vars`"));
        let plain = node.span.byte_range().len() == text.len();
        if !plain {
            return Some(diagnostic);
        }
        let edit = TextEdit::replace(node.span, as_vars(text, &found));
        Some(diagnostic.fix(Fix::suggested("Write each placeholder as a var", vec![edit])))
    }
}

/// Each `{name}` placeholder in `text`, as where its `{` is and its name, in order; a `${…}`
/// template is not one.
fn placeholders(text: &str) -> Vec<(usize, &str)> {
    let opens = text.match_indices('{').filter(|(at, _)| !text.get(..*at).is_some_and(|before| before.ends_with('$')));
    let found = opens.filter_map(|(at, _)| Some((at, text.get(at + 1..)?.split_once('}')?.0)));
    found.filter(|(_, name)| is_name(name)).collect()
}

/// Whether `name` could be a var's name: a letter, then letters, digits and `_`.
fn is_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `text` with each of its `placeholders` written as `${var.name}`.
fn as_vars(text: &str, placeholders: &[(usize, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for (at, name) in placeholders {
        out.push_str(text.get(from..*at).unwrap_or_default());
        out.push_str("${var.");
        out.push_str(name);
        out.push('}');
        from = at + name.len() + 2;
    }
    out.push_str(text.get(from..).unwrap_or_default());
    out
}

#[cfg(test)]
mod tests {
    use super::{as_vars, placeholders};

    #[test]
    fn a_placeholder_is_a_braced_name_outside_a_template() {
        let names = |text| placeholders(text).into_iter().map(|(_, name)| name).collect::<Vec<_>>();
        assert_eq!(names("/poll/{jobId}/runs/{run_2}"), ["jobId", "run_2"]);
        assert_eq!(names("/users/${var.id}/{ x }/{}/{1a}/{open"), Vec::<&str>::new());
        let mixed = "/a/${id}/{id}/{id}x";
        assert_eq!(as_vars(mixed, &placeholders(mixed)), "/a/${id}/${var.id}/${var.id}x");
    }
}
