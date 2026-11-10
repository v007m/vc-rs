use std::path::Path;
use std::sync::{Arc, Barrier};

use super::folder;

#[test]
fn a_folder_that_would_leave_home_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let error = folder(home.path(), Path::new("plugin-logs/../../elsewhere")).unwrap_err();
    assert!(error.to_string().contains("` leaves `"), "{error}");
}

#[test]
fn a_folder_already_there_is_used() {
    let home = tempfile::tempdir().unwrap();
    let first = folder(home.path(), Path::new(".vibranium/plugin-logs/suite")).unwrap();
    let again = folder(home.path(), Path::new(".vibranium/plugin-logs/suite")).unwrap();
    assert_eq!((first.is_dir(), again), (true, home.path().join(".vibranium/plugin-logs/suite")));
}

#[test]
fn hosts_making_the_same_folders_at_once_all_get_them() {
    let home = Arc::new(tempfile::tempdir().unwrap());
    let together = Arc::new(Barrier::new(8));
    let walks: Vec<_> = (0..8)
        .map(|_| {
            let (home, together) = (Arc::clone(&home), Arc::clone(&together));
            std::thread::spawn(move || {
                together.wait();
                folder(home.path(), Path::new(".vibranium/plugin-logs/suite")).map_err(|error| error.to_string())
            })
        })
        .collect();
    for walk in walks {
        let walked = walk.join().unwrap();
        assert!(walked.is_ok(), "no host is refused a folder another just made: {walked:?}");
    }
}

#[test]
fn a_file_on_the_way_is_refused() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".vibranium"), "not a folder").unwrap();
    let error = folder(home.path(), Path::new(".vibranium/plugin-data")).unwrap_err();
    assert!(error.to_string().ends_with("` is a link or not a folder"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_linked_folder_on_the_way_is_refused_and_nothing_is_made_where_it_points() {
    let (home, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(elsewhere.path(), home.path().join(".vibranium")).unwrap();
    let error = folder(home.path(), Path::new(".vibranium/plugin-data/suite")).unwrap_err();
    assert!(error.to_string().ends_with("` is a link or not a folder"), "{error}");
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0, "nothing made there");
}
