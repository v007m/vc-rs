use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{apply_fixes, byte_offset, complete, explain, hover};
use crate::Code;
use crate::engine_fixture::{API, CRUD, SECRET, TESTS, project};

fn target(target: &str) -> wire::ExplainRequest {
    wire::ExplainRequest { target: target.into(), ..wire::ExplainRequest::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_code_in_any_case_explains_as_its_markdown_page() {
    let (_dir, api) = project();
    let page = explain(api.clone(), target("v2003")).await.unwrap();
    assert!(page.markdown && page.text.starts_with("# V2003"), "{page:?}");
    let unknown = explain(api, target("V9999")).await.unwrap_err();
    assert_eq!(unknown.code, Code::NotFound, "{unknown:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_api_explains_as_a_run_sees_it_with_its_values_masked() {
    let (_dir, api) = project();
    let shown = explain(api.clone(), target("users#list")).await.unwrap();
    assert!(!shown.markdown && shown.text.contains("/users"), "{shown:?}");
    assert!(!format!("{shown:?}").contains(SECRET), "{shown:?}");
    let missing = explain(api, target("users#nothing")).await.unwrap_err();
    assert_eq!(missing.code, Code::InvalidArgument, "{missing:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_certain_fixes_preview_then_write_with_the_previews_digest() {
    let (dir, api) = project();
    let misnamed = CRUD.replace("listsUsers", "listUsers");
    fs::write(dir.path().join(TESTS), &misnamed).unwrap();
    let preview = apply_fixes(api.clone(), wire::ApplyFixesRequest::default()).await.unwrap();
    let [file] = preview.files.as_slice() else { panic!("{preview:?}") };
    assert!(file.path == TESTS && file.after.contains("id: listsEveryUser"), "{file:?}");
    assert!(!preview.titles.is_empty(), "{preview:?}");
    assert_eq!(fs::read_to_string(dir.path().join(TESTS)).unwrap(), misnamed, "a preview writes nothing");
    let stale = wire::ApplyFixesRequest { write: true, ..wire::ApplyFixesRequest::default() };
    assert_eq!(apply_fixes(api.clone(), stale).await.unwrap_err().code, Code::Aborted);
    let write = wire::ApplyFixesRequest {
        write: true,
        expected_digest: Some(preview.digest),
        ..wire::ApplyFixesRequest::default()
    };
    apply_fixes(api, write).await.unwrap();
    assert!(fs::read_to_string(dir.path().join(TESTS)).unwrap().contains("id: listsEveryUser"));
}

#[tokio::test(flavor = "multi_thread")]
async fn fixes_are_limited_to_the_codes_asked_and_a_bad_code_is_refused_by_its_place() {
    let (dir, api) = project();
    fs::write(dir.path().join(TESTS), CRUD.replace("listsUsers", "listUsers")).unwrap();
    let other = wire::ApplyFixesRequest { codes: vec!["v4002".into()], ..wire::ApplyFixesRequest::default() };
    assert_eq!(apply_fixes(api.clone(), other).await.unwrap().files, Vec::new());
    let bad =
        wire::ApplyFixesRequest { codes: vec!["V4001".into(), "../etc".into()], ..wire::ApplyFixesRequest::default() };
    let refused = apply_fixes(api, bad).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
    assert!(refused.message.contains("code 2") && !refused.message.contains("etc"), "{refused:?}");
}

/// The text with `^` taken out, and the cursor where it was in UTF-16 code units.
fn cursor_in(text: &str) -> (String, u32) {
    let before = text.find('^').unwrap();
    (text.replace('^', ""), u32::try_from(text[..before].encode_utf16().count()).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn completion_counts_the_cursor_as_a_browser_does_and_offers_var_names_only() {
    let (_dir, api) = project();
    let (text, offset) = cursor_in("tests:\n  - id: a\n    name: 'é 😀'\n    ca^\n");
    let request = wire::CompleteRequest { path: TESTS.into(), text, offset };
    let found = complete(api.clone(), request).await.unwrap();
    let [call] = found.items.as_slice() else { panic!("{found:?}") };
    assert_eq!((call.label.as_str(), call.kind()), ("call", wire::CompletionKind::Key));
    let (text, offset) = cursor_in("apis:\n  - id: a\n    path: /x?n=${var.no^\n");
    let vars = complete(api, wire::CompleteRequest { path: API.into(), text, offset }).await.unwrap();
    let [note] = vars.items.as_slice() else { panic!("{vars:?}") };
    assert_eq!((note.label.as_str(), note.detail.as_str()), ("var.note", "config"));
    assert!(!format!("{vars:?}").contains(SECRET), "{vars:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn hover_shows_a_keys_docs_and_the_api_a_call_names_masked() {
    let (_dir, api) = project();
    let hovering = |text: &str| {
        let (text, offset) = cursor_in(text);
        wire::HoverRequest { path: TESTS.into(), text, offset, ..wire::HoverRequest::default() }
    };
    let key = hover(api.clone(), hovering("tests:\n  - id: a\n    ta^gs: [x]\n")).await.unwrap();
    assert!(key.markdown.as_deref().is_some_and(|text| text.starts_with("**`tags`**")), "{key:?}");
    let call = hover(api.clone(), hovering("tests:\n  - id: a\n    call: users#li^st\n")).await.unwrap();
    let shown = call.markdown.unwrap_or_default();
    assert!(shown.starts_with("**`call`**") && shown.contains("```text\nusers#list  List users"), "{shown}");
    assert!(!shown.contains(SECRET), "{shown}");
    let unknown = hover(api.clone(), hovering("tests:\n  - id: a\n    call: users#no^ne\n")).await.unwrap();
    assert!(unknown.markdown.is_some_and(|text| !text.contains("```")), "the key's docs alone");
    assert_eq!(hover(api, hovering("tests:\n  - id: a\n    nothing^: 1\n")).await.unwrap().markdown, None);
}

#[test]
fn a_utf16_offset_lands_on_a_character_boundary() {
    assert_eq!(byte_offset("aé😀b", 0), 0);
    assert_eq!(byte_offset("aé😀b", 2), 3);
    assert_eq!(byte_offset("aé😀b", 3), 7, "inside a surrogate pair is after it");
    assert_eq!(byte_offset("aé😀b", 4), 7);
    assert_eq!(byte_offset("aé😀b", 99), 8);
}
