use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{apply_fixes, explain};
use crate::Code;
use crate::engine_fixture::{CRUD, SECRET, TESTS, project};

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
