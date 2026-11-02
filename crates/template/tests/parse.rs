#![cfg(test)]

use proptest::prelude::*;
use serde_json::json;
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Namespace, Part, Pos, Reference, Segment, SourceFileId, SourceSpan, StringTable, Template};
use vc_template::{parse, root_name};

const LINE: u32 = 2;
const COLUMN: u32 = 5;
const OFFSET: usize = 20;
const NO_ARGS: [serde_json::Value; 0] = [];

/// The span of a one-line scalar written with `quotes` quote characters on each side.
fn scalar(text: &str, quotes: usize) -> SourceSpan {
    let end = Pos::new(
        LINE,
        COLUMN + u32::try_from(text.chars().count() + 2 * quotes).unwrap(),
        OFFSET + text.len() + 2 * quotes,
    );
    SourceSpan::new(SourceFileId::new(0), Pos::new(LINE, COLUMN, OFFSET), end)
}

/// Columns `from..to` (0-based in the text) of a plain scalar on the test line.
fn cols(from: u32, to: u32) -> SourceSpan {
    let at = |c: u32| Pos::new(LINE, COLUMN + c, OFFSET + c as usize);
    SourceSpan::new(SourceFileId::new(0), at(from), at(to))
}

fn ok(text: &str) -> Template {
    parse(text, scalar(text, 0), &mut StringTable::new()).unwrap()
}

fn err(text: &str) -> Diagnostic {
    *parse(text, scalar(text, 0), &mut StringTable::new()).unwrap_err()
}

fn only_ref(text: &str) -> Reference {
    ok(text).single_ref().cloned().unwrap()
}

fn keys(names: &[&str]) -> Vec<Segment> {
    names.iter().map(|k| Segment::Key((*k).to_owned())).collect()
}

#[test]
fn literal_text_is_one_part() {
    assert_eq!(ok("hello world").parts, [Part::Literal("hello world".into())]);
    assert_eq!(ok("").parts, []);
    assert!(!ok("hello").preserves_type());
}

#[test]
fn a_lone_reference_preserves_its_type() {
    let template = ok("${var.userId}");
    assert!(template.preserves_type());
    let reference = template.single_ref().unwrap();
    assert_eq!((reference.ns, reference.path.segments.clone()), (Namespace::Var, keys(&["userId"])));
    assert_eq!(reference.span, cols(0, 13));
}

#[test]
fn mixed_text_and_references_become_a_string() {
    let template = ok("user-${run.id}-x");
    assert!(!template.preserves_type());
    assert!(matches!(&template.parts[..], [Part::Literal(a), Part::Ref(r), Part::Literal(b)]
        if a == "user-" && b == "-x" && r.ns == Namespace::Run && r.span == cols(5, 14)));
}

#[test]
fn paths_take_keys_indexes_stars_and_quoted_keys() {
    let reference = only_ref(r#"${response.body.items[-1][*]["x-id"]['a b'][0]}"#);
    let mut expected = keys(&["body", "items"]);
    expected.extend([Segment::Index(-1), Segment::All, Segment::Key("x-id".into()), Segment::Key("a b".into())]);
    expected.push(Segment::Index(0));
    assert_eq!((reference.ns, reference.path.segments), (Namespace::Response, expected));
    assert_eq!(only_ref("${dataset}").path.segments, []);
}

#[test]
fn pipes_keep_their_arguments_and_spans() {
    let reference = only_ref("${ var.note | default('none') | take(10) | acme.mask }");
    let names: Vec<_> = reference.pipes.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["default", "take", "acme.mask"]);
    assert_eq!(reference.pipes[0].args, [json!("none")]);
    assert_eq!(reference.pipes[1].args, [json!(10)]);
    assert_eq!((reference.pipes[1].name_span, reference.pipes[1].span), (cols(32, 36), cols(32, 40)));
    assert_eq!(reference.pipes[2].args, NO_ARGS);
    let literals = only_ref(r#"${var.x | f(true, false, null, -2.5, "q")}"#);
    assert_eq!(literals.pipes[0].args, [json!(true), json!(false), json!(null), json!(-2.5), json!("q")]);
    assert_eq!(only_ref("${var.x|trim()}").pipes[0].args, NO_ARGS);
}

#[test]
fn generators_take_literal_arguments() {
    let reference = only_ref("${fake.number.int(1, 100)}");
    assert_eq!((reference.ns, reference.args.clone()), (Namespace::Fake, vec![json!(1), json!(100)]));
    assert_eq!(reference.path.segments, keys(&["number", "int"]));
    assert_eq!(only_ref("${fake.lorem.word}").args, NO_ARGS);
}

#[test]
fn escapes_and_lone_dollars_are_literal() {
    assert_eq!(ok("$${var.x}").parts, [Part::Literal("${var.x}".into())]);
    assert_eq!(ok("cost $5, a$$b $").parts, [Part::Literal("cost $5, a$$b $".into())]);
    let parts = ok("$${a} ${var.b}").parts;
    assert!(matches!(&parts[..], [Part::Literal(text), Part::Ref(r)] if text == "${a} " && r.span == cols(6, 14)));
}

#[test]
fn other_roots_are_step_ids() {
    let mut strings = StringTable::new();
    let template = parse("${place.body.total}", scalar("${place.body.total}", 0), &mut strings).unwrap();
    let Namespace::Step(id) = template.single_ref().unwrap().ns else { panic!("expected a step") };
    assert_eq!(strings.resolve(id), Some("place"));
    assert_eq!(root_name(Namespace::Step(id)), None);
    assert_eq!(root_name(Namespace::Iteration), Some("iteration"));
}

#[test]
fn every_fixed_root_parses_to_its_namespace() {
    for root in
        ["var", "env", "secret", "response", "request", "example", "item", "iteration", "fake", "dataset", "run"]
    {
        assert_eq!(root_name(only_ref(&format!("${{{root}}}")).ns), Some(root));
    }
}

#[test]
fn syntax_errors_point_at_the_character_where_parsing_stopped() {
    let cases = [
        ("${var.x", 7, 7),
        ("${}", 2, 3),
        ("${ }", 3, 4),
        ("${var.}", 6, 7),
        ("${1abc}", 2, 3),
        ("${var.a[}", 8, 9),
        ("${var.a[x]}", 8, 9),
        (r#"${var.a["x}"#, 11, 11),
        ("${var.x | }", 10, 11),
        ("${var.x | take(}", 15, 16),
        ("${var.x | default(foo)}", 18, 19),
        ("ok ${var.x y}", 11, 12),
    ];
    for (text, from, to) in cases {
        let diagnostic = err(text);
        assert_eq!((diagnostic.code, diagnostic.primary.span), (Code::V2305, cols(from, to)), "{text}");
        assert!(diagnostic.message.starts_with("invalid template"), "{text}");
    }
}

#[test]
fn only_generators_take_arguments() {
    let diagnostic = err("a ${var.x(1)}");
    assert_eq!((diagnostic.code, diagnostic.primary.span), (Code::V2305, cols(2, 13)));
    assert!(diagnostic.message.contains("`var` takes no arguments"));
}

#[test]
fn quoted_scalars_shift_by_the_quote_and_others_map_to_the_whole_scalar() {
    let text = "${var.x}";
    let quoted = parse(text, scalar(text, 1), &mut StringTable::new()).unwrap();
    assert_eq!(quoted.single_ref().unwrap().span, cols(1, 9));
    let block =
        SourceSpan::new(SourceFileId::new(0), Pos::new(LINE, COLUMN, OFFSET), Pos::new(LINE, COLUMN + 30, OFFSET + 30));
    let escaped = parse(text, block, &mut StringTable::new()).unwrap();
    assert_eq!(escaped.single_ref().unwrap().span, block);
    let lines = SourceSpan::new(block.file, block.start, Pos::new(LINE + 1, 1, OFFSET + text.len()));
    let folded = parse(text, lines, &mut StringTable::new()).unwrap();
    assert_eq!(folded.single_ref().unwrap().span, lines);
}

#[test]
fn columns_count_characters_and_offsets_count_bytes() {
    let text = "é${var.x}";
    let Some(Part::Ref(reference)) = ok(text).parts.pop() else { panic!("expected a reference") };
    let span = reference.span;
    assert_eq!(
        (span.start, span.end),
        (Pos::new(LINE, COLUMN + 1, OFFSET + 2), Pos::new(LINE, COLUMN + 9, OFFSET + 10))
    );
    let unclosed = err("é${");
    assert_eq!(unclosed.primary.span.start, Pos::new(LINE, COLUMN + 3, OFFSET + 4));
}

proptest! {
    #[test]
    fn parse_never_panics(text in "\\PC{0,60}", quotes in 0usize..3) {
        let _ = parse(&text, scalar(&text, quotes), &mut StringTable::new());
    }

    #[test]
    fn parse_never_panics_on_template_like_text(text in r#"[$${}a-z.|()'"\[\]*0-9, -]{0,40}"#) {
        let _ = parse(&text, scalar(&text, 0), &mut StringTable::new());
    }
}
