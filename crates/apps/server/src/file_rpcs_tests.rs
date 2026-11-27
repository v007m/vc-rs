use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{create_file, file_references, format, rename, trash_file};
use crate::Code;
use crate::engine_fixture::{API, CRUD, TESTS, project};

#[tokio::test]
async fn create_file_previews_the_template_then_writes_it_with_the_preview_digest() {
    let (dir, api) = project();
    let request = wire::CreateFileRequest { kind: "test".into(), id: "users.pages".into(), ..Default::default() };
    let preview = create_file(api.clone(), request.clone()).await.unwrap();
    assert_eq!(preview.path, "services/users/pages.test.yaml");
    assert!(preview.files[0].before.is_none() && !dir.path().join(&preview.path).exists(), "{preview:?}");
    let write = wire::CreateFileRequest { write: true, expected_digest: Some(preview.digest), ..request.clone() };
    create_file(api.clone(), write).await.unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(&preview.path)).unwrap(), preview.files[0].after);
    let again = create_file(api.clone(), request).await.unwrap_err();
    assert_eq!(again.code, Code::InvalidArgument);
    let config = wire::CreateFileRequest { kind: "config".into(), id: "x".into(), ..Default::default() };
    assert_eq!(create_file(api, config).await.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test]
async fn rename_previews_every_file_it_changes_and_a_stale_write_is_aborted() {
    let (dir, api) = project();
    let request = wire::RenameRequest { from: "users#list".into(), to: "users#listAll".into(), ..Default::default() };
    let preview = rename(api.clone(), request.clone()).await.unwrap();
    let paths: Vec<&str> = preview.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, vec![TESTS, API]);
    let stale = wire::RenameRequest { write: true, expected_digest: Some("0".into()), ..request.clone() };
    assert_eq!(rename(api.clone(), stale).await.unwrap_err().code, Code::Aborted);
    assert_eq!(fs::read_to_string(dir.path().join(TESTS)).unwrap(), CRUD);
    let write = wire::RenameRequest { write: true, expected_digest: Some(preview.digest), ..request };
    rename(api, write).await.unwrap();
    assert!(fs::read_to_string(dir.path().join(TESTS)).unwrap().contains("call: users#listAll"));
}

#[tokio::test]
async fn format_gives_the_house_style_unsaved() {
    let (dir, api) = project();
    let text = "tests:\n  - call: users#list\n    id: listsUsers\n";
    let request = wire::FormatRequest { path: TESTS.into(), text: text.into(), strip_comments: false };
    let formatted = format(api.clone(), request).await.unwrap();
    assert_eq!(
        (formatted.text.as_str(), formatted.changed),
        ("tests:\n  - id: listsUsers\n    call: users#list\n", true)
    );
    assert_eq!(fs::read_to_string(dir.path().join(TESTS)).unwrap(), CRUD);
    let hidden = wire::FormatRequest { path: ".env".into(), text: "A=1\n".into(), strip_comments: false };
    assert_eq!(format(api, hidden).await.unwrap_err().code, Code::InvalidArgument);
}

#[tokio::test]
async fn trash_lists_the_references_first_and_refuses_a_stale_or_hidden_file() {
    let (dir, api) = project();
    let found = file_references(api.clone(), wire::FileReferencesRequest { path: API.into() }).await.unwrap();
    let reference = wire::FileReference {
        path: TESTS.into(),
        line: 5,
        from: "users.crud#listsUsers".into(),
        target: "users#list".into(),
    };
    assert_eq!((found.references, found.unread), (vec![reference], Vec::new()));
    let stale = wire::TrashFileRequest { path: TESTS.into(), expected_digest: "0".into() };
    assert_eq!(trash_file(api.clone(), stale).await.unwrap_err().code, Code::Aborted);
    assert_eq!(fs::read_to_string(dir.path().join(TESTS)).unwrap(), CRUD);
    let missing = wire::TrashFileRequest { path: "services/users/none.test.yaml".into(), expected_digest: "0".into() };
    assert_eq!(trash_file(api.clone(), missing).await.unwrap_err().code, Code::NotFound);
    let env = wire::TrashFileRequest { path: ".env".into(), expected_digest: String::new() };
    assert_eq!(trash_file(api, env).await.unwrap_err().code, Code::InvalidArgument);
}
