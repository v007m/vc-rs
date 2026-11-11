//! `syntaxVersion` (D54): a config written for a newer file format, or for one before the first
//! this compiler reads, is V1003. A missing one is V1003 too, reported by the shape-check from
//! the key's declaration.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Node, ScalarKind};

/// The newest file format this compiler reads.
pub const SUPPORTED: i64 = 2;

/// The first file format of Vibranium v2; v1 projects have no `syntaxVersion`.
const FIRST: i64 = 2;

/// V1003 when the config's `syntaxVersion` is newer than [`SUPPORTED`] or older than the first.
pub fn check(root: &Node) -> Option<Diagnostic> {
    let node = root.get("syntaxVersion")?;
    let version: i64 = node.as_scalar().filter(|s| s.kind == ScalarKind::Int)?.text.parse().ok()?;
    if version < FIRST {
        let message = format!("`syntaxVersion: {version}` is not a file format; the first is {FIRST}");
        let diagnostic = Diagnostic::new(Code::V1003, node.span, message).label(format!("older than {FIRST}"));
        return Some(diagnostic.help(format!(
            "write `syntaxVersion: {SUPPORTED}`; to convert a Vibranium v1 project, run `vc migrate v1 <folder>`"
        )));
    }
    (version > SUPPORTED).then(|| {
        let message = format!("`syntaxVersion: {version}` is newer than this Vibranium reads");
        let diagnostic = Diagnostic::new(Code::V1003, node.span, message).label(format!("reads up to {SUPPORTED}"));
        diagnostic.help("upgrade vc to read this project")
    })
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::Code;
    use vc_model::SourceFileId;
    use vc_parser::Syntax;

    use super::check;

    fn root(text: &str) -> vc_parser::Node {
        vc_parser::parse(SourceFileId::new(0), Syntax::Yaml, text).0.unwrap()
    }

    #[test]
    fn flags_a_newer_version_and_one_before_the_first() {
        let newer = check(&root("syntaxVersion: 3\n")).unwrap();
        assert_eq!(newer.code, Code::V1003);
        assert_eq!(newer.message, "`syntaxVersion: 3` is newer than this Vibranium reads");
        assert!(check(&root("syntaxVersion: 2\n")).is_none());
        for older in ["0", "1", "-2"] {
            let found = check(&root(&format!("syntaxVersion: {older}\n"))).unwrap();
            assert_eq!(found.code, Code::V1003);
            assert_eq!(found.message, format!("`syntaxVersion: {older}` is not a file format; the first is 2"));
        }
        assert!(check(&root("syntaxVersion: '9'\n")).is_none());
        assert!(check(&root("project: x\n")).is_none());
    }
}
