//! `syntaxVersion` (D54): a config written for a newer file format is V1003. A missing one is
//! V1003 too, reported by the shape-check from the key's declaration.

use vc_diagnostics::{Code, Diagnostic};
use vc_parser::{Node, ScalarKind};

/// The newest file format this compiler reads.
pub const SUPPORTED: i64 = 2;

/// V1003 when the config's `syntaxVersion` is newer than [`SUPPORTED`].
pub fn check(root: &Node) -> Option<Diagnostic> {
    let node = root.get("syntaxVersion")?;
    let version: i64 = node.as_scalar().filter(|s| s.kind == ScalarKind::Int)?.text.parse().ok()?;
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
    fn flags_only_a_newer_version() {
        let newer = check(&root("syntaxVersion: 3\n")).unwrap();
        assert_eq!(newer.code, Code::V1003);
        assert_eq!(newer.message, "`syntaxVersion: 3` is newer than this Vibranium reads");
        assert!(check(&root("syntaxVersion: 2\n")).is_none());
        assert!(check(&root("syntaxVersion: '9'\n")).is_none());
        assert!(check(&root("project: x\n")).is_none());
    }
}
