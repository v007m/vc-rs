use std::fs;

use vc_interfaces::{SourceError, SourceWriter, content_hash};
use vc_model::SourcePath;

use super::FsSources;

const TEXT: &str = "tests: []\n";
const PATH: &str = "services/users/crud.test.yaml";

fn project() -> (tempfile::TempDir, FsSources, SourcePath) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join(PATH), TEXT).unwrap();
    let sources = FsSources::new(dir.path()).unwrap();
    (dir, sources, SourcePath::new(PATH).unwrap())
}

#[test]
fn a_trashed_file_keeps_its_path_under_a_fresh_trash_folder_and_a_changed_one_stays() {
    let (dir, sources, path) = project();
    let changed = sources.trash(&path, content_hash(b"other"));
    assert!(matches!(changed, Err(SourceError::Changed { .. })), "{changed:?}");
    assert!(dir.path().join(PATH).exists());
    let first = sources.trash(&path, content_hash(TEXT.as_bytes())).unwrap();
    fs::write(dir.path().join(PATH), TEXT).unwrap();
    let second = sources.trash(&path, content_hash(TEXT.as_bytes())).unwrap();
    assert_ne!(first, second);
    for place in [first, second] {
        assert!(place.starts_with(".vibranium/trash/") && place.ends_with(PATH), "{place}");
        assert_eq!(fs::read_to_string(dir.path().join(place)).unwrap(), TEXT);
    }
    assert!(!dir.path().join(PATH).exists());
}

#[cfg(unix)]
#[test]
fn a_trash_folder_that_links_out_of_the_project_is_refused() {
    let (dir, sources, path) = project();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".vibranium")).unwrap();
    let refused = sources.trash(&path, content_hash(TEXT.as_bytes()));
    assert!(matches!(refused, Err(SourceError::OutsideRoot { .. })), "{refused:?}");
    assert!(dir.path().join(PATH).exists());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
