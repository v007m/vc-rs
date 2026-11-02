use proptest::prelude::*;
use vc_model::SourceFileId;
use vc_parser::{Document, parse};

const YAMLISH: &str = "[a-z: \\-\\[\\]{},#&*!|>'\"\n\t\r\u{feff}é?%@`\\\\]{0,64}";

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
    fn every_certain_fix_still_parses(text in YAMLISH) {
        let Ok(document) = Document::parse(SourceFileId::new(0), text.as_str()) else { return Ok(()) };
        let (_, diagnostics) = parse(SourceFileId::new(0), &text);
        for fix in diagnostics.iter().flat_map(|d| &d.fixes).filter(|fix| fix.certain) {
            prop_assert!(document.apply(&fix.edits).is_ok(), "{:?}", fix.edits);
        }
    }
}
