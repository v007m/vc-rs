use proptest::prelude::*;
use vc_model::SourceFileId;
use vc_parser::{Document, Node, parse};

const YAMLISH: &str = "[a-z: \\-\\[\\]{},#&*!|>'\"\n\t\r\u{feff}é?%@`\\\\]{0,64}";
const VALUE: &str = "[a-z0-9 ,:#'\"\\[\\]{}\\-.~é]{0,12}";

/// Every scalar node under `root`.
fn scalars(root: &Node) -> Vec<&Node> {
    let mut stack = vec![root];
    let mut found = Vec::new();
    while let Some(node) = stack.pop() {
        if node.as_scalar().is_some() {
            found.push(node);
        }
        stack.extend(node.items());
        stack.extend(node.entries().iter().map(|entry| &entry.value));
    }
    found
}

proptest! {
    #[test]
    fn parse_never_panics_on_any_text(text in any::<String>()) {
        let _ = parse(SourceFileId::new(0), &text);
    }

    #[test]
    fn parse_never_panics_on_yaml_shaped_text(text in YAMLISH) {
        let (root, diagnostics) = parse(SourceFileId::new(0), &text);
        prop_assert!(root.is_some() || diagnostics.has_errors());
        for diagnostic in diagnostics.iter() {
            prop_assert!(text.get(diagnostic.primary.span.byte_range()).is_some());
        }
    }

    #[test]
    fn every_fix_still_parses(text in YAMLISH) {
        let Ok(document) = Document::parse(SourceFileId::new(0), text.as_str()) else { return Ok(()) };
        let (_, diagnostics) = parse(SourceFileId::new(0), &text);
        for fix in diagnostics.iter().flat_map(|d| &d.fixes) {
            prop_assert!(document.apply(&fix.edits).is_ok(), "{:?}", fix.edits);
        }
    }

    #[test]
    fn every_scalar_edit_still_parses(text in YAMLISH, value in VALUE) {
        let Ok(document) = Document::parse(SourceFileId::new(0), text.as_str()) else { return Ok(()) };
        for node in scalars(document.root()) {
            if let Ok(edit) = document.set_scalar(node, &value) {
                prop_assert!(document.apply(std::slice::from_ref(&edit)).is_ok(), "{text:?} {edit:?}");
            }
        }
    }
}
