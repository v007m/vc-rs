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
