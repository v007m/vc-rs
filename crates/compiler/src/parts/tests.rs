use vc_diagnostics::{Code, Diagnostics};
use vc_model::{FileKind, Matcher, MatcherArg, ScalarStyle, SourceFileId, StatusPattern, StringTable, TemplateValue};

use super::{Piece, Tools, style};
use crate::stage::FileUnit;

fn unit(kind: FileKind, text: &str) -> FileUnit {
    let path = match kind {
        FileKind::Config => "vibranium.config.yaml",
        FileKind::Api => "services/users/users.api.yaml",
        FileKind::Test => "services/users/crud.test.yaml",
        FileKind::Load => "load/peak.load.yaml",
    };
    let path = vc_model::SourcePath::new(path).unwrap();
    let file = vc_model::SourceFile { path, text: text.to_owned(), hash: vc_model::ContentHash::new([0; 32]) };
    crate::stage::unit(SourceFileId::new(0), &file, (&Tools::new(), &mut StringTable::new()))
}

fn codes(diagnostics: &Diagnostics) -> Vec<Code> {
    diagnostics.iter().map(|d| d.code).collect()
}

fn pieces(unit: &FileUnit) -> Vec<&Piece> {
    unit.parts.values().collect()
}

fn test(body: &str) -> String {
    format!("tests:\n  - id: getsUser\n    call: users#get\n{body}")
}

#[test]
fn compiles_templates_and_checks_their_roots() {
    let ok = unit(FileKind::Api, "baseUrl: ${var.url | trim}\n");
    assert!(ok.diagnostics.is_empty(), "{:?}", ok.diagnostics);
    assert!(matches!(pieces(&ok)[..], [Piece::Template(_)]));
    let response = unit(FileKind::Api, "baseUrl: ${response.body}\n");
    assert_eq!(codes(&response.diagnostics), [Code::V2105]);
    let pipe = unit(FileKind::Api, "baseUrl: ${var.url | nope}\n");
    assert_eq!(codes(&pipe.diagnostics).len(), 1);
}

#[test]
fn leaves_step_roots_to_the_linker_except_bare_response_fields() {
    let step = unit(FileKind::Test, &test("    vars: {a: '${login.body.token}'}\n"));
    assert!(step.diagnostics.is_empty(), "{:?}", step.diagnostics);
    let bare = unit(FileKind::Test, &test("    vars: {a: '${body.token}'}\n"));
    assert_eq!(codes(&bare.diagnostics), [Code::V2105]);
}

#[test]
fn reads_units_and_statuses() {
    let text = test("    timeout: 2s\n    expect:\n      status: [200, 2xx]\n      size: {lt: 10kb}\n");
    let unit = unit(FileKind::Test, &text);
    assert!(unit.diagnostics.is_empty(), "{:?}", unit.diagnostics);
    let found = pieces(&unit);
    assert!(matches!(found[0], Piece::Duration(_)));
    let Piece::Matcher(Matcher::Status(statuses)) = found[1] else { panic!("{:?}", found[1]) };
    assert_eq!(statuses, &[StatusPattern::Code(200), StatusPattern::Class(2)]);
    let Piece::Matcher(Matcher::All(calls)) = found[2] else { panic!("{:?}", found[2]) };
    assert!(matches!(calls[0].arg, MatcherArg::Size(_)));
    let bad = unit_with("    timeout: soon\n    expect: {status: 99x}\n");
    assert_eq!(codes(&bad.diagnostics).len(), 2);
    let templated = unit_with("    timeout: ${var.wait}\n");
    assert!(matches!(pieces(&templated)[..], [Piece::Template(_)]));
}

fn unit_with(body: &str) -> FileUnit {
    unit(FileKind::Test, &test(body))
}

#[test]
fn checks_matcher_arguments() {
    let ok = unit_with(
        "    expect:\n      body:\n        match:\n          id: {type: string, length: {gt: 3}}\n          tags: {every: {in: [a, b]}}\n          n: {between: [1, 5], not: {eq: 3}}\n          s: {regex: '^a+$', anyOf: [x, {startsWith: y}]}\n",
    );
    assert!(ok.diagnostics.is_empty(), "{:?}", ok.diagnostics);
    assert_eq!(ok.parts.len(), 4);
    let bad = unit_with(
        "    expect:\n      body:\n        match:\n          a: {type: strng}\n          b: {between: [1]}\n          c: {regex: '(' }\n          d: {exists: maybe}\n          e: {in: x}\n          f: {length: long}\n          g: {allOf: x}\n",
    );
    assert_eq!(codes(&bad.diagnostics), [Code::V2306; 7]);
    assert_eq!(bad.diagnostics.iter().next().unwrap().help, ["did you mean `string`?"]);
    let unknown = unit_with("    expect:\n      headers: {x-id: {eq: 1, gtt: 2}}\n");
    assert_eq!(codes(&unknown.diagnostics), [Code::V1004]);
}

#[test]
fn reads_ranges_and_lengths_in_their_unit() {
    let timing = unit_with("    expect:\n      timing: {total: {between: [1s, 2s]}}\n");
    assert!(timing.diagnostics.is_empty(), "{:?}", timing.diagnostics);
    let Piece::Matcher(Matcher::All(calls)) = pieces(&timing)[0] else { panic!("{:?}", pieces(&timing)) };
    let MatcherArg::Range(bounds) = &calls[0].arg else { panic!("{:?}", calls[0].arg) };
    assert!(matches!(bounds.as_ref(), [MatcherArg::Duration(_), MatcherArg::Duration(_)]), "{bounds:?}");
    let lengths = unit_with(
        "    expect:\n      body:\n        match:\n          a: {length: 3}\n          b: {length: '${var.n}'}\n",
    );
    assert!(lengths.diagnostics.is_empty(), "{:?}", lengths.diagnostics);
    assert_eq!(lengths.parts.len(), 2);
}

#[test]
fn checks_cel_with_the_files_action_ids() {
    let text = "beforeAll:\n  - id: seed\n    command: ./seed\ntests:\n  - id: a\n    call: users#get\n    vars: {n: {$cel: 'seed.count + 1'}}\n    expect:\n      cel: {big: response.body.n > 1}\n";
    let unit = unit(FileKind::Test, text);
    assert!(unit.diagnostics.is_empty(), "{:?}", unit.diagnostics);
    let found = pieces(&unit);
    assert!(matches!(found[0], Piece::Template(_)), "the command");
    assert!(matches!(found[1], Piece::Value(TemplateValue::Cel(_))), "{:?}", found[1]);
    assert!(matches!(found[2], Piece::Cel(_)));
    let unknown = unit_with("    vars: {n: {$cel: 'other.count'}}\n");
    assert_eq!(codes(&unknown.diagnostics), [Code::V2105]);
}

#[test]
fn lowers_values_with_their_types() {
    let unit = unit_with("    body: {a: 1, b: 2.5, c: true, d: null, e: [x], f: '0x10'}\n");
    let Piece::Value(TemplateValue::Map(body)) = pieces(&unit)[0] else { panic!() };
    assert_eq!(body["a"], TemplateValue::Number(1.into()));
    assert_eq!(body["c"], TemplateValue::Bool(true));
    assert_eq!(body["d"], TemplateValue::Null);
    assert!(matches!(body["e"], TemplateValue::List(_)));
    assert!(matches!(body["f"], TemplateValue::String(_)));
}

#[test]
fn maps_scalar_styles_for_exact_spans() {
    use vc_parser::ScalarStyle as Parsed;
    assert_eq!(style(Parsed::Plain, "a"), ScalarStyle::Plain);
    assert_eq!(style(Parsed::SingleQuoted, "'it''s'"), ScalarStyle::SingleQuoted { escaped: true });
    assert_eq!(style(Parsed::SingleQuoted, "''"), ScalarStyle::SingleQuoted { escaped: false });
    assert_eq!(style(Parsed::DoubleQuoted, "\"a\\n\""), ScalarStyle::DoubleQuoted { escaped: true });
    assert_eq!(style(Parsed::DoubleQuoted, "\"a\""), ScalarStyle::DoubleQuoted { escaped: false });
    assert_eq!(style(Parsed::Literal, "|\n a"), ScalarStyle::Block);
    assert_eq!(style(Parsed::Folded, ">\n a"), ScalarStyle::Block);
}
