//! `reports.dir` in the project config (V2313, D327): a folder inside the project, written as
//! plain folder names. A run writes its reports there and prunes old run folders from it, so an
//! absolute path or a `..` part would let a cloned project write and remove files anywhere. A
//! folder outside the project is the user's to choose (`--out`, `--set`, `VIB_REPORTS_DIR` or
//! the user config), which the compiler never sees. The check reads `/` and `\` alike, so a
//! project checked on one OS is safe on another.

use vc_diagnostics::{Code, Diagnostic, Diagnostics};
use vc_parser::Node;

use super::project::{Project, Source};

/// Checks the config's `reports.dir`.
pub fn check(project: &Project<'_>) -> Diagnostics {
    let Some(config) = project.config() else { return Diagnostics::default() };
    let dir = config.root.get("reports").and_then(|reports| reports.get("dir"));
    dir.and_then(|node| leaving(config, node)).into_iter().collect()
}

/// V2313 when `node`, the written `reports.dir`, is absolute or goes up with `..`.
fn leaving(config: &Source<'_>, node: &Node) -> Option<Diagnostic> {
    let written = node.as_str()?;
    let label = why(written)?;
    let message = format!("`reports.dir` is `{written}`, which leaves the project");
    let help = format!(
        "`reports.dir` at {}:{} is the project's, and stays inside it: write a folder such as `reports/ci`, \
         or pass `--out <dir>` for one outside it",
        config.path, node.span.start.line
    );
    Some(Diagnostic::new(Code::V2313, node.span, message).label(label).help(help))
}

/// Why `written` leaves the project: an absolute path, a drive, or a `..` part; `None` when it
/// stays inside.
fn why(written: &str) -> Option<&'static str> {
    if written.starts_with(['/', '\\']) || written.as_bytes().get(1) == Some(&b':') {
        return Some("an absolute path");
    }
    written.split(['/', '\\']).any(|part| part == "..").then_some("`..` leads out of the project")
}

#[cfg(test)]
mod tests {
    use super::why;

    #[test]
    fn only_plain_folder_names_stay_inside() {
        for inside in ["reports/ci", "./out", ".vibranium/reports", "a..b/c", "reports\\ci"] {
            assert_eq!(why(inside), None, "{inside}");
        }
        for absolute in ["/tmp/out", "\\\\server\\share", "C:\\out", "c:out"] {
            assert_eq!(why(absolute), Some("an absolute path"), "{absolute}");
        }
        for up in ["../out", "a/../../b", "..", "a\\..\\..\\b"] {
            assert_eq!(why(up), Some("`..` leads out of the project"), "{up}");
        }
    }
}
