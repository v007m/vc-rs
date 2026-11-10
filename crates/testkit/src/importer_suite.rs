use std::collections::HashSet;

use vc_interfaces::{ImportDraft, ImportError, ImportOptions, ImportSource, Importer};

/// The contract every [`Importer`] meets (D112).
///
/// `sample` is a file of the importer's kind holding at least one api. The draft for it is the
/// same on every read and names every api once, with a camelCase ID and a path that starts with
/// `/`. Bytes that are not of the kind are an [`ImportError::Invalid`] naming the location,
/// never a panic.
///
/// # Panics
///
/// When the importer breaks the contract.
pub async fn importer_suite(importer: &dyn Importer, sample: &ImportSource) {
    let opts = ImportOptions { file_id: "suite".to_owned(), ..ImportOptions::default() };
    let draft = importer.read(sample, &opts).await;
    assert!(draft.is_ok(), "the sample reads: {:?}", draft.as_ref().err());
    let draft = draft.unwrap_or_default();
    names_every_api_once(&draft);
    assert_eq!(importer.read(sample, &opts).await.ok(), Some(draft), "the same draft on every read");
    for bytes in [&b""[..], b"\x00\x01 not a file of this kind", b"[1, 2"] {
        let garbage = ImportSource { location: "garbage.txt".to_owned(), bytes: bytes.to_vec() };
        let result = importer.read(&garbage, &opts).await;
        let invalid = matches!(&result, Err(ImportError::Invalid { location, .. }) if location == "garbage.txt");
        assert!(invalid, "{bytes:?} is invalid at its location: {result:?}");
    }
}

fn names_every_api_once(draft: &ImportDraft) {
    assert!(draft.groups.iter().any(|group| !group.apis.is_empty()), "the sample has apis");
    for group in &draft.groups {
        let mut seen = HashSet::new();
        for api in &group.apis {
            assert!(seen.insert(&api.id), "{}#{} is named once", group.file_id, api.id);
            assert!(camel_case(&api.id), "{:?} is camelCase", api.id);
            assert!(api.path.starts_with('/'), "{:?} starts with /", api.path);
        }
    }
}

fn camel_case(id: &str) -> bool {
    id.starts_with(|first: char| first.is_ascii_lowercase()) && id.chars().all(|c| c.is_ascii_alphanumeric())
}
