use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_interfaces::{SourceError, SourceWriter, content_hash};
use vc_model::SourcePath;

use super::FsSources;
use crate::Bin;

const TEXT: &str = "tests: []\n";
const PATH: &str = "services/users/crud.test.yaml";

/// A bin that keeps each file in its own folder, or, when `full`, takes none.
#[derive(Debug)]
struct Kept {
    folder: tempfile::TempDir,
    full: bool,
}

impl Bin for Kept {
    fn put(&self, file: &Path) -> std::io::Result<String> {
        if self.full {
            return Err(std::io::Error::other("no trash here"));
        }
        let to = self.folder.path().join(file.file_name().unwrap_or_default());
        fs::rename(file, &to)?;
        Ok(to.display().to_string())
    }
}

fn project(full: bool) -> (tempfile::TempDir, FsSources, SourcePath) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join(PATH), TEXT).unwrap();
    let bin = Arc::new(Kept { folder: tempfile::tempdir().unwrap(), full });
    let sources = FsSources::new(dir.path()).unwrap().with_bin(bin);
    (dir, sources, SourcePath::new(PATH).unwrap())
}

#[test]
fn a_trashed_file_goes_to_the_bin_which_names_where_and_a_changed_one_stays() {
    let (dir, sources, path) = project(false);
    let changed = sources.trash(&path, content_hash(b"other"));
    assert!(matches!(changed, Err(SourceError::Changed { .. })), "{changed:?}");
    assert!(dir.path().join(PATH).exists());
    let place = sources.trash(&path, content_hash(TEXT.as_bytes())).unwrap();
    assert!(place.ends_with("crud.test.yaml"), "{place}");
    assert_eq!(fs::read_to_string(&place).unwrap(), TEXT);
    assert!(!dir.path().join(PATH).exists());
}

#[test]
fn a_file_the_bin_does_not_take_is_refused_and_left_in_the_project() {
    let (dir, sources, path) = project(true);
    let refused = sources.trash(&path, content_hash(TEXT.as_bytes()));
    assert_eq!(refused, Err(SourceError::Untrashed { path: PATH.into() }));
    assert_eq!(fs::read_to_string(dir.path().join(PATH)).unwrap(), TEXT);
}

#[cfg(unix)]
#[test]
fn a_file_through_a_folder_that_links_out_of_the_project_is_refused() {
    let (dir, sources, _) = project(false);
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("crud.test.yaml"), TEXT).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("services/linked")).unwrap();
    let linked = SourcePath::new("services/linked/crud.test.yaml").unwrap();
    let refused = sources.trash(&linked, content_hash(TEXT.as_bytes()));
    assert!(matches!(refused, Err(SourceError::OutsideRoot { .. })), "{refused:?}");
    assert!(outside.path().join("crud.test.yaml").exists());
}

/// Sends a scratch file to this machine's real trash; run it by hand with `--ignored`.
#[test]
#[ignore = "moves a scratch file into this machine's own trash"]
fn the_system_trash_takes_a_file_and_it_leaves_the_project() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("services/users")).unwrap();
    fs::write(dir.path().join(PATH), TEXT).unwrap();
    let sources = FsSources::new(dir.path()).unwrap();
    let path = SourcePath::new(PATH).unwrap();
    assert!(sources.trash(&path, content_hash(TEXT.as_bytes())).is_ok());
    assert!(!dir.path().join(PATH).exists());
}
