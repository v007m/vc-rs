use ScalarKind::{Bool, Float, Int, Null, String};
use vc_diagnostics::{Code, Diagnostics};
use vc_model::{Pos, SourceFileId, SourceSpan};
use vc_parser::{Node, ScalarKind, ScalarStyle, parse};

fn ok(text: &str) -> Option<Node> {
    let (root, diagnostics) = parse(SourceFileId::new(0), text);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    root
}

type Place = (u32, u32, usize);

fn at(span: SourceSpan) -> (Place, Place) {
    let tuple = |p: Pos| (p.line, p.column, p.offset);
    (tuple(span.start), tuple(span.end))
}

fn codes(diagnostics: &Diagnostics) -> Vec<Code> {
    diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn nested_maps_and_sequences_have_exact_spans() {
    let root = ok("a: 1\nb:\n  - x\n  - y: z\n").unwrap();
    let b = root.entry("b").unwrap();
    assert_eq!(at(root.entries()[0].key_span), ((1, 1, 0), (1, 2, 1)));
    assert_eq!(at(root.get("a").unwrap().span), ((1, 4, 3), (1, 5, 4)));
    assert_eq!(at(b.key_span), ((2, 1, 5), (2, 2, 6)));
    let items = b.value.items();
    assert_eq!(at(items[0].span), ((3, 5, 12), (3, 6, 13)));
    assert_eq!(at(items[1].span), ((4, 5, 18), (4, 9, 22)));
    assert_eq!(at(items[1].entries()[0].key_span), ((4, 5, 18), (4, 6, 19)));
    assert_eq!(at(root.span), ((1, 1, 0), (4, 9, 22)));
}

#[test]
fn columns_count_characters_and_offsets_count_bytes() {
    let root = ok("név: 'ő'\nb: é\n").unwrap();
    assert_eq!(at(root.entries()[0].key_span), ((1, 1, 0), (1, 4, 4)));
    assert_eq!(at(root.get("név").unwrap().span), ((1, 6, 6), (1, 9, 10)));
    assert_eq!(at(root.get("b").unwrap().span), ((2, 4, 14), (2, 5, 16)));
    let with_bom = ok("\u{feff}a: 1\r\nb: 2\r\n").unwrap();
    assert_eq!(at(with_bom.entries()[0].key_span), ((1, 1, 3), (1, 2, 4)));
    assert_eq!(at(with_bom.get("b").unwrap().span), ((2, 4, 12), (2, 5, 13)));
}

#[test]
fn keeps_key_order_styles_and_flow_spans() {
    let root = ok("z: {x: 1}\nb: [1]\nc: |\n  x\nd: 'q'\ne: \"q\"\nf: >\n  f\ng: plain\n").unwrap();
    let keys: Vec<_> = root.entries().iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["z", "b", "c", "d", "e", "f", "g"]);
    assert_eq!(at(root.get("z").unwrap().span), ((1, 4, 3), (1, 10, 9)));
    assert_eq!(at(root.get("b").unwrap().span), ((2, 4, 13), (2, 7, 16)));
    let style = |key| root.get(key).unwrap().as_scalar().unwrap().style;
    let styles = ["c", "d", "e", "f", "g"].map(style);
    let expected = [ScalarStyle::Literal, ScalarStyle::SingleQuoted, ScalarStyle::DoubleQuoted, ScalarStyle::Folded];
    assert_eq!(styles[..4], expected);
    assert_eq!(styles[4], ScalarStyle::Plain);
    assert_eq!(root.get("c").unwrap().as_str(), Some("x\n"));
}

#[test]
fn scalars_resolve_by_the_core_schema() {
    let root = ok("n: ~\nb: true\ni: 0x1F\nf: 1.5\ns: '12'\nt: 12ms\ne:\n").unwrap();
    let kind = |key| root.get(key).unwrap().as_scalar().unwrap().kind;
    let kinds = ["n", "b", "i", "f", "s", "t", "e"].map(kind);
    assert_eq!(kinds, [Null, Bool, Int, Float, String, String, Null]);
    assert_eq!(root.get("i").unwrap().as_scalar().unwrap().text, "0x1F");
    assert_eq!(root.get("i").unwrap().as_str(), None);
}

#[test]
fn json_goes_through_the_same_tree() {
    let root = ok(r#"{"a":-0,"d":"x\"y\u00e9","e":{},"f":[1, "two", null],"h i":true}"#).unwrap();
    assert_eq!(root.get("d").unwrap().as_str(), Some("x\"yé"));
    assert_eq!(at(root.get("d").unwrap().span), ((1, 13, 12), (1, 25, 24)));
    assert_eq!(root.get("f").unwrap().items().len(), 3);
    assert_eq!(root.get("h i").unwrap().as_scalar().unwrap().kind, ScalarKind::Bool);
    assert_eq!(ok("[1]\n").unwrap().items().len(), 1);
}

#[test]
fn json_surrogate_pair_escapes_are_a_known_saphyr_gap() {
    let (root, diagnostics) = parse(SourceFileId::new(0), r#"{"a": "\ud83d\ude00"}"#);
    assert!(root.is_none());
    assert_eq!(codes(&diagnostics), [Code::V1001]);
}

#[test]
fn an_empty_file_is_a_null_root() {
    let root = ok("# only a comment\n").unwrap();
    assert_eq!(root.as_scalar().unwrap().kind, ScalarKind::Null);
}

#[test]
fn broken_syntax_is_v1001_and_drops_the_tree() {
    let (root, diagnostics) = parse(SourceFileId::new(0), "a: \"unterminated\n");
    assert!(root.is_none());
    let error = diagnostics.iter().next().unwrap();
    assert_eq!((error.code, at(error.primary.span)), (Code::V1001, ((1, 4, 3), (1, 5, 4))));
    assert_eq!(error.help.len(), 1);
}

#[test]
fn indentation_and_mapping_errors_are_v1002() {
    for (text, line) in [("a: b: c\n", 1), ("a:\n  b: 1\n c: 2\n", 3), ("- a\nb: 1\n", 2)] {
        let (root, diagnostics) = parse(SourceFileId::new(0), text);
        assert!(root.is_none(), "{text:?}");
        assert_eq!(codes(&diagnostics), [Code::V1002], "{text:?}");
        assert_eq!(diagnostics.iter().next().unwrap().primary.span.start.line, line);
    }
}

#[test]
fn every_duplicate_key_is_reported_with_both_places() {
    let (root, diagnostics) = parse(SourceFileId::new(0), "a: 1\nb: 2\na: 3\nb: 4\n");
    assert_eq!(codes(&diagnostics), [Code::V1002, Code::V1002]);
    let first = diagnostics.iter().next().unwrap();
    assert_eq!(at(first.primary.span), ((3, 1, 10), (3, 2, 11)));
    assert_eq!(at(first.secondary[0].span), ((1, 1, 0), (1, 2, 1)));
    assert_eq!(root.unwrap().get("a").unwrap().as_scalar().unwrap().text, "1");
}

#[test]
fn aliases_tags_complex_keys_and_second_documents_are_v1002() {
    let cases = [
        ("a: &x 1\nb: *x\n", ((2, 4, 11), (2, 6, 13))),
        ("a: !!str 1\n", ((1, 4, 3), (1, 11, 10))),
        ("? [a]\n: 1\n", ((1, 3, 2), (1, 6, 5))),
        ("a: 1\n---\nb: 2\n", ((2, 1, 5), (2, 4, 8))),
    ];
    for (text, span) in cases {
        let (root, diagnostics) = parse(SourceFileId::new(0), text);
        assert!(root.is_some(), "{text:?}");
        assert_eq!(codes(&diagnostics), [Code::V1002], "{text:?}");
        assert_eq!(at(diagnostics.iter().next().unwrap().primary.span), span, "{text:?}");
    }
    assert!(parse(SourceFileId::new(0), "a: &x 1\n").1.is_empty());
    assert_eq!(codes(&parse(SourceFileId::new(0), "a: *missing\n").1), [Code::V1001]);
}

#[test]
fn nesting_deeper_than_the_limit_is_refused() {
    let text = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let (root, diagnostics) = parse(SourceFileId::new(0), &text);
    assert!(root.is_none());
    assert_eq!(codes(&diagnostics), [Code::V1002]);
}

#[test]
fn a_value_cut_at_a_hash_is_v3204_with_a_certain_quote_fix() {
    let text = "reason: coupon flaky, see #212\nnext: ok  # fine\nlist: a, # b\n";
    let (root, diagnostics) = parse(SourceFileId::new(0), text);
    assert_eq!(root.unwrap().get("reason").unwrap().as_str(), Some("coupon flaky, see"));
    assert_eq!(codes(&diagnostics), [Code::V3204, Code::V3204]);
    let cut = diagnostics.iter().next().unwrap();
    assert_eq!(at(cut.primary.span), ((1, 9, 8), (1, 31, 30)));
    let fix = &cut.fixes[0];
    assert!(fix.certain);
    assert_eq!((fix.edits[0].range.clone(), fix.edits[0].new_text.as_str()), (8..30, "\"coupon flaky, see #212\""));
    assert_eq!(diagnostics.iter().nth(1).unwrap().fixes[0].edits[0].new_text, "\"a, # b\"");
}
