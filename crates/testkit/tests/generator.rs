//! The project generator is deterministic, sized exactly, and writes files that parse cleanly.

use std::collections::BTreeSet;

use vc_model::SourceFileId;
use vc_testkit::generate_project;

#[test]
fn the_same_seed_gives_the_same_project() {
    assert_eq!(generate_project(42, 60), generate_project(42, 60));
    assert_ne!(generate_project(42, 60), generate_project(43, 60));
}

#[test]
fn projects_have_exactly_the_files_asked_for() {
    for size in [0, 1, 2, 5, 6, 7, 1000] {
        let project = generate_project(1, size);
        assert_eq!(project.files().len(), size, "size {size}");
        let paths: BTreeSet<_> = project.files().iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths.len(), size, "paths are unique for size {size}");
    }
    let project = generate_project(1, 7);
    let paths: Vec<_> = project.files().iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths[..3],
        ["vibranium.config.yaml", "services/accounts/accounts.api.yaml", "services/accounts/crud.test.yaml"]
    );
    assert_eq!(paths[6], "services/orders/orders.api.yaml");
}

#[test]
fn every_file_parses_without_diagnostics() {
    for (index, file) in generate_project(7, 300).files().iter().enumerate() {
        let id = SourceFileId::new(u32::try_from(index).unwrap());
        let (root, diagnostics) = vc_parser::parse(id, &file.text);
        assert!(root.is_some() && diagnostics.is_empty(), "{}: {diagnostics:?}\n{}", file.path, file.text);
    }
}

#[test]
fn test_files_reuse_the_crud_flow_and_call_other_services() {
    let project = generate_project(3, 40);
    let text = |path: &str| project.files().iter().find(|file| file.path == path).map(|file| file.text.clone()).unwrap();
    assert!(text("services/accounts/crud.test.yaml").contains("id: preparesItem"));
    assert!(text("services/accounts/search.test.yaml").contains("test: accounts.crud#preparesItem"));
    assert!(text("vibranium.config.yaml").contains("call: accounts#create"));
    assert!(project.files().iter().all(|file| !file.text.contains('%')), "every placeholder is filled");
}

#[test]
fn one_changed_file_differs_only_in_the_last_file() {
    let project = generate_project(5, 20);
    let changed = project.with_one_file_changed();
    let differing: Vec<_> =
        project.files().iter().zip(changed.files()).filter(|(a, b)| a != b).map(|(a, _)| a.path.clone()).collect();
    assert_eq!(differing, [project.files()[19].path.clone()]);
    assert!(generate_project(5, 0).with_one_file_changed().files().is_empty());
}
