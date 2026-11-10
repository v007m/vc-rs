#![cfg(test)]

use proptest::prelude::proptest;
use serde_json::json;
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{
    ModelError, Namespace, Part, Pos, Reference, ScalarSource, ScalarStyle, Segment, SourceFileId, SourceSpan,
    StringTable, Template,
};
use vc_template::TemplateError;

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

fn plain(text: &str) -> ScalarSource {
    ScalarSource { span: scalar(text, 0), style: ScalarStyle::Plain }
}

fn ok(text: &str) -> Template {
    vc_template::parse(text, plain(text), &mut StringTable::new()).unwrap()
}

fn syntax(result: Result<Template, TemplateError>) -> Diagnostic {
    match result {
        Err(TemplateError::Syntax(diagnostic)) => *diagnostic,
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

fn err(text: &str) -> Diagnostic {
    syntax(vc_template::parse(text, plain(text), &mut StringTable::new()))
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
    let literals = only_ref("${var.x | f(true, false, -2.5, 'q', '')}");
    assert_eq!(literals.pipes[0].args, [json!(true), json!(false), json!(-2.5), json!("q"), json!("")]);
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
    let template = vc_template::parse("${place.body.total}", plain("${place.body.total}"), &mut strings).unwrap();
    let Namespace::Step(id) = template.single_ref().unwrap().ns else { panic!("expected a step") };
    assert_eq!(strings.resolve(id), Some("place"));
    assert_eq!(vc_template::root_name(Namespace::Step(id)), None);
    assert_eq!(vc_template::root_name(Namespace::Iteration), Some("iteration"));
}

#[test]
fn every_fixed_root_parses_to_its_namespace() {
    for root in
        ["var", "env", "secret", "response", "request", "example", "item", "iteration", "fake", "dataset", "run"]
    {
        assert_eq!(vc_template::root_name(only_ref(&format!("${{{root}}}")).ns), Some(root));
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
        ("${var.a[0}", 9, 10),
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
    let span_of =
        |span, style| vc_template::parse(text, ScalarSource { span, style }, &mut StringTable::new()).unwrap();
    let span_of = |span, style| span_of(span, style).single_ref().unwrap().span;
    for style in [ScalarStyle::DoubleQuoted { escaped: false }, ScalarStyle::SingleQuoted { escaped: false }] {
        assert_eq!(span_of(scalar(text, 1), style), cols(1, 9), "{style:?}");
    }
    for style in
        [ScalarStyle::DoubleQuoted { escaped: true }, ScalarStyle::SingleQuoted { escaped: true }, ScalarStyle::Block]
    {
        assert_eq!(span_of(scalar(text, 1), style), scalar(text, 1), "{style:?}");
    }
    assert_eq!(span_of(scalar(text, 1), ScalarStyle::Plain), scalar(text, 1));
    let lines =
        SourceSpan::new(SourceFileId::new(0), scalar(text, 0).start, Pos::new(LINE + 1, 1, OFFSET + text.len()));
    assert_eq!(span_of(lines, ScalarStyle::Plain), lines);
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

#[test]
fn errors_say_what_the_innermost_rule_expected() {
    let cases = [
        ("${var.a[0}", "expected `]`"),
        ("${var.a[x]}", "expected an index, `*` or a quoted key"),
        ("${var.}", "expected a key after `.`"),
        ("${}", "expected a root such as `var`"),
        ("${var.x y}", "expected `}` or `|`"),
        ("${var.x | }", "expected a pipe name"),
        ("${var.x | take(}", "expected a literal, `,` or `)`"),
        ("${var.x | take(007)}", "expected a number without leading zeros"),
        ("${var.x | take(1.)}", "expected a digit after `.`"),
        ("${var.x | take(-)}", "expected a digit"),
        ("${var.x | take(1e)}", "expected a digit in the exponent"),
        ("${var.x | take(1e999)}", "expected a number in range"),
        (r#"${var.a["x}"#, "expected a closing `\"`"),
    ];
    for (text, expected) in cases {
        assert_eq!(err(text).message, format!("invalid template: {expected}"), "{text}");
    }
}

#[test]
fn literal_numbers_are_json_numbers() {
    let args = |text: &str| only_ref(&format!("${{var.x | f({text})}}")).pipes[0].args.clone();
    assert_eq!(args("0, 12, -7"), [json!(0), json!(12), json!(-7)]);
    assert_eq!(
        args("0.5, -2.25, 1e3, 2E-2, 1.5e+2"),
        [json!(0.5), json!(-2.25), json!(1000.0), json!(0.02), json!(150.0)]
    );
}

#[test]
fn bare_status_headers_and_body_are_step_ids_and_roots_have_spans() {
    let mut strings = StringTable::new();
    let template = vc_template::parse("${body.items[0]}", plain("${body.items[0]}"), &mut strings).unwrap();
    let reference = template.single_ref().unwrap();
    let Namespace::Step(id) = reference.ns else { panic!("expected a step") };
    assert_eq!(strings.resolve(id), Some("body"));
    assert_eq!(reference.path.segments, [keys(&["items"]), vec![Segment::Index(0)]].concat());
    assert_eq!(reference.root_span, cols(2, 6));
    let Some(Part::Ref(spaced)) = ok("a ${ var.x }").parts.pop() else { panic!("expected a reference") };
    assert_eq!(spaced.root_span, cols(5, 8));
}

#[test]
fn null_and_double_quoted_arguments_are_v2305() {
    let text = r#"${var.x | default("none")}"#;
    let diagnostic = err(text);
    assert_eq!((diagnostic.code, diagnostic.primary.span), (Code::V2305, cols(18, 24)));
    assert_eq!(diagnostic.message, "invalid template: text arguments take single quotes");
    assert_eq!(diagnostic.help, ["use single quotes"]);
    let [fix] = &diagnostic.fixes[..] else { panic!("expected one fix") };
    assert!(fix.certain);
    assert_eq!((fix.edits[0].new_text.as_str(), fix.edits[0].range.clone()), ("'none'", 38..44));
    let null = err("${var.x | default(null)}");
    assert_eq!(
        (null.code, null.primary.span, null.message.as_str()),
        (Code::V2305, cols(18, 22), "invalid template: `null` is not an argument")
    );
    assert_eq!((null.help.len(), null.fixes.len()), (1, 0));
    assert_eq!(err(r#"${fake.helpers.arrayElement('a', "b")}"#).code, Code::V2305);
}

#[test]
fn double_quoted_arguments_get_no_fix_when_single_quotes_would_break() {
    for text in [r#"${var.x | default("it's")}"#, r#"${var.x | default("a\b")}"#] {
        assert_eq!(err(text).fixes.len(), 0, "{text}");
    }
    let text = r#"${var.x | default("none")}"#;
    for style in [ScalarStyle::SingleQuoted { escaped: false }, ScalarStyle::DoubleQuoted { escaped: true }] {
        let diagnostic =
            syntax(vc_template::parse(text, ScalarSource { span: scalar(text, 1), style }, &mut StringTable::new()));
        assert_eq!((diagnostic.code, diagnostic.fixes.len()), (Code::V2305, 0), "{style:?}");
    }
    let quoted = ScalarSource { span: scalar(text, 1), style: ScalarStyle::DoubleQuoted { escaped: false } };
    assert_eq!(syntax(vc_template::parse(text, quoted, &mut StringTable::new())).fixes.len(), 1);
}

#[test]
fn a_full_string_table_is_internal_not_syntax() {
    let error = TemplateError::from(ModelError::StringTableFull(3));
    assert!(matches!(error, TemplateError::Internal(ModelError::StringTableFull(3))));
    assert_eq!(error.to_string(), ModelError::StringTableFull(3).to_string());
    let syntax = TemplateError::Syntax(Box::new(err("${")));
    assert_eq!(syntax.to_string(), err("${").message);
}

proptest! {
    #[test]
    fn parse_never_panics(text in "\\PC{0,60}", quotes in 0usize..3) {
        let style = if quotes == 0 { ScalarStyle::Plain } else { ScalarStyle::DoubleQuoted { escaped: quotes > 1 } };
        let _ = vc_template::parse(&text, ScalarSource { span: scalar(&text, quotes), style }, &mut StringTable::new());
    }

    #[test]
    fn parse_never_panics_on_template_like_text(text in r#"[$${}a-z.|()'"\[\]*0-9, -]{0,40}"#) {
        let _ = vc_template::parse(&text, plain(&text), &mut StringTable::new());
    }
}
