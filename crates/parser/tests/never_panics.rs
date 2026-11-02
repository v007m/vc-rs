use proptest::prelude::*;
use vc_diagnostics::Code;
use vc_model::SourceFileId;
use vc_parser::{Document, Node, Syntax, parse};

const YAMLISH: &str = "[a-z: \\-\\[\\]{},#&*!|>'\"\n\t\r\u{feff}é?%@`\\\\]{0,64}";
const VALUE: &str = "[a-z0-9 ,:#'\"\\[\\]{}\\-.~é]{0,12}";
const JSONISH: &str = "[\\[\\]{}:, \"\\\\/a-fnrtu0-9.eE+\\-\n\r\té😀]{0,64}";

/// Valid JSON documents, nested a few levels deep.
fn json_documents() -> impl Strategy<Value = String> {
    let leaf = prop_oneof![
        Just("null".to_owned()),
        Just("true".to_owned()),
        "-?(0|[1-9][0-9]{0,3})(\\.[0-9]{1,2})?([eE][+-]?[0-9])?",
        "[a-z é,:#{}\\[\\]'~]{0,6}".prop_map(|text| format!("\"{text}\"")),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(|items| format!("[{}]", items.join(", "))),
            prop::collection::vec(("[a-c]", inner), 0..4).prop_map(|entries| {
                let entries: Vec<_> = entries.iter().map(|(key, value)| format!("\"{key}\": {value}")).collect();
                format!("{{\n  {}\n}}", entries.join(",\n  "))
            }),
        ]
    })
}

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
        let _ = parse(SourceFileId::new(0), Syntax::Yaml, &text);
    }

    #[test]
    fn parse_never_panics_on_yaml_shaped_text(text in YAMLISH) {
        let (root, diagnostics) = parse(SourceFileId::new(0), Syntax::Yaml, &text);
        prop_assert!(root.is_some() || diagnostics.has_errors());
        for diagnostic in diagnostics.iter() {
            prop_assert!(text.get(diagnostic.primary.span.byte_range()).is_some());
        }
    }

    #[test]
    fn every_fix_still_parses(text in YAMLISH) {
        let Ok(document) = Document::parse(SourceFileId::new(0), Syntax::Yaml, text.as_str()) else { return Ok(()) };
        let (_, diagnostics) = parse(SourceFileId::new(0), Syntax::Yaml, &text);
        for fix in diagnostics.iter().flat_map(|d| &d.fixes) {
            prop_assert!(document.apply(&fix.edits).is_ok(), "{:?}", fix.edits);
        }
    }

    #[test]
    fn every_scalar_edit_still_parses(text in YAMLISH, value in VALUE) {
        every_edit_parses(Syntax::Yaml, &text, &value)?;
    }

    #[test]
    fn the_json_reader_never_panics_on_any_text(text in any::<String>()) {
        let _ = parse(SourceFileId::new(0), Syntax::Json, &text);
    }

    #[test]
    fn the_json_reader_never_panics_on_json_shaped_text(text in JSONISH) {
        let (root, diagnostics) = parse(SourceFileId::new(0), Syntax::Json, &text);
        prop_assert!(root.is_some() || diagnostics.has_errors(), "{text:?}");
        for diagnostic in diagnostics.iter() {
            prop_assert!(text.get(diagnostic.primary.span.byte_range()).is_some());
        }
    }

    #[test]
    fn valid_json_reads_with_at_most_duplicate_keys(text in json_documents()) {
        let (root, diagnostics) = parse(SourceFileId::new(0), Syntax::Json, &text);
        prop_assert!(root.is_some(), "{text:?}");
        prop_assert!(diagnostics.iter().all(|d| d.code == Code::V1006), "{text:?}: {diagnostics:?}");
    }

    #[test]
    fn every_json_scalar_edit_still_parses(text in json_documents(), value in VALUE) {
        every_edit_parses(Syntax::Json, &text, &value)?;
    }
}

/// Every scalar edit to `text` that is accepted must give text that parses again.
fn every_edit_parses(syntax: Syntax, text: &str, value: &str) -> Result<(), TestCaseError> {
    let Ok(document) = Document::parse(SourceFileId::new(0), syntax, text) else { return Ok(()) };
    for node in scalars(document.root()) {
        if let Ok(edit) = document.set_scalar(node, value) {
            prop_assert!(document.apply(std::slice::from_ref(&edit)).is_ok(), "{text:?} {edit:?}");
        }
    }
    Ok(())
}
