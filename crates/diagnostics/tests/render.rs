//! Snapshot tests for the text and JSON renderers on hand-built diagnostics.

use vc_diagnostics::{
    Code, Diagnostic, Diagnostics, DiagnosticsError, Fix, JsonRenderer, Render, TextEdit, TextRenderer,
};
use vc_model::{ContentHash, SourceFile, SourceFileId, SourceFiles, SourcePath, SourceSpan};

const TEST_FILE: &str = "\
id: orders.checkout
tests:
  - id: placesOrder
    call: orders#plcae
    profiles: [stagng]
    body:
      items:
        - sku: A1
          qty: 2
      coupon: SAVE10
";

const CONFIG_FILE: &str = "\
syntaxVersion: 2
profiles:
  - id: staging
";

const TEST: SourceFileId = SourceFileId::new(0);
const CONFIG: SourceFileId = SourceFileId::new(1);

fn files() -> Result<SourceFiles, vc_model::ModelError> {
    let mut files = SourceFiles::new();
    for (path, text) in [("services/orders/checkout.test.yaml", TEST_FILE), ("vibranium.config.yaml", CONFIG_FILE)] {
        let file = SourceFile { path: SourcePath::new(path)?, text: text.to_owned(), hash: ContentHash::new([0; 32]) };
        files.push(file)?;
    }
    Ok(files)
}

/// The span from the start of `from` to the end of `to`, both found in file `id`.
fn span(files: &SourceFiles, id: SourceFileId, from: &str, to: &str) -> Option<SourceSpan> {
    let text = &files.get(id)?.text;
    let start = text.find(from)?;
    let end = start + text[start..].find(to)? + to.len();
    Some(SourceSpan::new(id, files.position(id, start)?, files.position(id, end)?))
}

fn diagnostics(files: &SourceFiles) -> Option<Diagnostics> {
    let call = span(files, TEST, "orders#plcae", "plcae")?;
    let profile = span(files, TEST, "stagng", "stagng")?;
    let staging = span(files, CONFIG, "staging", "staging")?;
    let items = span(files, TEST, "items:", "qty: 2")?;
    let body = span(files, TEST, "body:", "SAVE10")?;
    let unknown_api = Diagnostic::new(Code::V2003, call, r#"unknown api "orders#plcae""#)
        .label("not defined")
        .help("did you mean orders#place?")
        .fix(Fix::certain("Replace with orders#place", vec![TextEdit::replace(call, "orders#place")]));
    let unknown_profile = Diagnostic::new(Code::V2002, profile, r#"unknown profile "stagng""#)
        .label("no such profile")
        .secondary(staging, "a close match is defined here")
        .note("profiles are defined in vibranium.config.yaml")
        .fix(Fix::suggested("Replace with staging", vec![TextEdit::replace(profile, "staging")]));
    let body_default = Diagnostic::new(Code::V3205, items, "a body default reaches a GET step")
        .label("sent with GET orders#list")
        .secondary(body, "test-level default set here");
    Some([unknown_api, unknown_profile, body_default].into_iter().collect())
}

fn one(diagnostic: Diagnostic) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    diagnostics.push(diagnostic);
    diagnostics
}

#[test]
fn text_shows_excerpts_labels_notes_and_fixes() {
    let files = files().unwrap();
    let text = TextRenderer::default().render(&diagnostics(&files).unwrap(), &files).unwrap();
    assert!(!text.contains('\x1b'), "colour must be off by default");
    insta::assert_snapshot!(text);
}

#[test]
fn text_colour_is_opt_in() {
    let files = files().unwrap();
    let text = TextRenderer::default().color(true).render(&diagnostics(&files).unwrap(), &files).unwrap();
    assert!(text.starts_with("\x1b[1;31merror[V2003]\x1b[0m: unknown api"));
    assert!(text.contains("\x1b[1;33mwarning[V3205]\x1b[0m"));
}

#[test]
fn json_resolves_spans_to_paths_and_counts_severities() {
    let files = files().unwrap();
    let json = JsonRenderer.render(&diagnostics(&files).unwrap(), &files).unwrap();
    insta::assert_snapshot!(json);
}

#[test]
fn spans_into_missing_files_are_an_error() {
    let files = files().unwrap();
    let elsewhere = SourceSpan { file: SourceFileId::new(7), ..span(&files, TEST, "id", "id").unwrap() };
    let mut diagnostics = Diagnostics::new();
    diagnostics.push(Diagnostic::new(Code::V2001, elsewhere, "duplicate"));
    let expected = Err(DiagnosticsError::UnknownFile(7));
    assert_eq!(TextRenderer::default().render(&diagnostics, &files), expected);
    assert_eq!(JsonRenderer.render(&diagnostics, &files), expected);
}

#[test]
fn nothing_to_report_renders_an_empty_summary() {
    let files = files().unwrap();
    let json = JsonRenderer.render(&Diagnostics::new(), &files).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let expected = serde_json::json!({"summary": {"errors": 0, "warnings": 0, "infos": 0}, "diagnostics": []});
    assert_eq!(value, expected);
    assert_eq!(TextRenderer::default().render(&Diagnostics::new(), &files).unwrap(), "");
}

#[test]
fn text_groups_excerpts_by_file_with_the_primary_file_first() {
    let files = files().unwrap();
    let call = span(&files, TEST, "orders#plcae", "plcae").unwrap();
    let staging = span(&files, CONFIG, "staging", "staging").unwrap();
    let coupon = span(&files, TEST, "coupon", "SAVE10").unwrap();
    let diagnostic = Diagnostic::new(Code::V2003, call, "unknown api")
        .label("not defined")
        .secondary(staging, "in the config")
        .secondary(coupon, "back in the test file");
    let text = TextRenderer::default().render(&one(diagnostic), &files).unwrap();
    insta::assert_snapshot!(text);
}

#[test]
fn text_stops_before_the_line_a_span_ends_at_the_start_of() {
    let files = files().unwrap();
    let text = &files.get(TEST).unwrap().text;
    let (start, end) = (text.find("    call").unwrap(), text.find("    profiles").unwrap());
    let line = SourceSpan::new(TEST, files.position(TEST, start).unwrap(), files.position(TEST, end).unwrap());
    let diagnostic = Diagnostic::new(Code::V2003, line, "unknown api").label("this line");
    let text = TextRenderer::default().render(&one(diagnostic), &files).unwrap();
    assert!(text.contains("4 |     call: orders#plcae\n  | ^^^^^^^^^^^^^^^^^^^^^^ this line\n"), "{text}");
    assert!(!text.contains("profiles"), "{text}");
}

#[test]
fn text_shows_control_characters_and_bidi_overrides_as_visible_stand_ins() {
    let mut files = SourceFiles::new();
    let path = SourcePath::new("services/evil/x.test.yaml").unwrap();
    let source = "name: \x1b[2Jboom\u{202e}gnp.exe\r\n";
    files.push(SourceFile { path, text: source.to_owned(), hash: ContentHash::new([0; 32]) }).unwrap();
    let id = SourceFileId::new(0);
    let name = span(&files, id, "\x1b", "boom").unwrap();
    let diagnostic = Diagnostic::new(Code::V2003, name, "bad \x1b]0;title\x07")
        .label("\u{2066}hidden\u{200f}")
        .note("a\rb")
        .help("\x7f\u{9b}")
        .fix(Fix::suggested("\x1b[1A", vec![TextEdit::replace(name, "ok")]));
    let text = TextRenderer::default().render(&one(diagnostic), &files).unwrap();
    let unsafe_chars = ['\x1b', '\r', '\x07', '\x7f', '\u{9b}', '\u{202e}', '\u{2066}', '\u{200f}'];
    assert!(!text.contains(unsafe_chars), "{text:?}");
    insta::assert_snapshot!(text);
}

#[test]
fn text_rejects_fix_edits_into_missing_files() {
    let files = files().unwrap();
    let call = span(&files, TEST, "orders#plcae", "plcae").unwrap();
    let elsewhere = SourceSpan { file: SourceFileId::new(9), ..call };
    let fix = Fix::certain("Replace", vec![TextEdit::replace(elsewhere, "x")]);
    let diagnostics = one(Diagnostic::new(Code::V2003, call, "unknown api").fix(fix));
    let expected = Err(DiagnosticsError::UnknownFile(9));
    assert_eq!(TextRenderer::default().render(&diagnostics, &files), expected);
    assert_eq!(JsonRenderer.render(&diagnostics, &files), expected);
}

#[test]
fn text_merges_excerpts_that_a_later_label_joins() {
    let files = files().unwrap();
    let call = span(&files, TEST, "call", "plcae").unwrap();
    let items = span(&files, TEST, "items", "items").unwrap();
    let bridge = span(&files, TEST, "profiles", "body").unwrap();
    let diagnostic = Diagnostic::new(Code::V2003, call, "unknown api")
        .label("first")
        .secondary(items, "second")
        .secondary(bridge, "joins them");
    let text = TextRenderer::default().render(&one(diagnostic), &files).unwrap();
    assert!(!text.contains("..."), "{text}");
    insta::assert_snapshot!(text);
}

#[test]
fn text_shows_a_single_line_between_labels_instead_of_eliding_it() {
    let files = files().unwrap();
    let call = span(&files, TEST, "call", "plcae").unwrap();
    let body = span(&files, TEST, "body", "body").unwrap();
    let diagnostic = Diagnostic::new(Code::V2003, call, "unknown api").label("here").secondary(body, "and here");
    let text = TextRenderer::default().render(&one(diagnostic), &files).unwrap();
    assert!(text.contains("\n5 |     profiles: [stagng]\n"), "{text}");
    assert!(!text.contains("..."), "{text}");
}

#[test]
fn a_masked_span_never_prints_and_the_underlines_after_it_move_to_match() {
    let mut files = SourceFiles::new();
    let path = SourcePath::new("services/users/users.api.yaml").unwrap();
    let source = "source:\n  headers:\n    Authorization: Bearer s3cr3t-t0ken # rotate\n";
    files.push(SourceFile { path, text: source.to_owned(), hash: ContentHash::new([0; 32]) }).unwrap();
    let id = SourceFileId::new(0);
    let key = span(&files, id, "Authorization", "Authorization").unwrap();
    let value = span(&files, id, "Bearer", "t0ken").unwrap();
    let diagnostic = Diagnostic::new(Code::V4009, key, "a literal credential")
        .label("kept in git")
        .secondary(span(&files, id, "rotate", "rotate").unwrap(), "rotate it")
        .masked(value);
    let diagnostics = one(diagnostic);
    let text = TextRenderer::default().render(&diagnostics, &files).unwrap();
    let expected = "3 |     Authorization: ••• # rotate\n  |     ^^^^^^^^^^^^^ kept in git\n  |                          ------ rotate it\n";
    assert!(text.contains(expected), "{text}");
    let json = JsonRenderer.render(&diagnostics, &files).unwrap();
    for shown in [&text, &json] {
        assert!(!["Bearer", "s3cr3t", "t0ken"].iter().any(|part| shown.contains(part)), "{shown}");
    }
}
