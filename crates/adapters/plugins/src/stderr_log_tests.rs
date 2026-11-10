use std::path::Path;

use super::{OLD_LOG, STDERR_LOG, log_folder, open};

#[test]
fn a_log_before_that_cannot_be_kept_is_removed_and_the_new_log_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(STDERR_LOG), "the start before").unwrap();
    std::fs::create_dir(dir.path().join(OLD_LOG)).unwrap();
    std::fs::write(dir.path().join(OLD_LOG).join("kept"), "a folder is in the way").unwrap();
    drop(open(dir.path(), Path::new("")).unwrap());
    let written = std::fs::read_to_string(dir.path().join(STDERR_LOG)).unwrap();
    assert!(written.starts_with("vibranium: the log before could not be kept as stderr.log.1 ("), "{written}");
    assert!(written.trim_end().ends_with("so it was removed"), "{written}");
}

#[test]
fn the_log_before_is_kept_as_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(STDERR_LOG), "the start before").unwrap();
    drop(open(dir.path(), Path::new("")).unwrap());
    assert_eq!(std::fs::read_to_string(dir.path().join(OLD_LOG)).unwrap(), "the start before");
    assert_eq!(std::fs::read_to_string(dir.path().join(STDERR_LOG)).unwrap(), "");
}

#[test]
fn the_folders_on_the_way_to_the_log_are_made() {
    let home = tempfile::tempdir().unwrap();
    drop(open(home.path(), Path::new("plugin-logs/suite")).unwrap());
    assert!(home.path().join("plugin-logs/suite").join(STDERR_LOG).is_file());
}

#[test]
fn a_folder_that_would_leave_home_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let error = log_folder(home.path(), Path::new("plugin-logs/../../elsewhere")).unwrap_err();
    assert!(error.to_string().contains("` leaves `"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_linked_log_folder_is_refused_and_what_it_points_at_is_untouched() {
    let (home, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(elsewhere.path().join(STDERR_LOG), "keep me").unwrap();
    std::fs::write(elsewhere.path().join(OLD_LOG), "keep me too").unwrap();
    std::fs::create_dir(home.path().join("plugin-logs")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), home.path().join("plugin-logs/suite")).unwrap();
    assert!(open(home.path(), Path::new("plugin-logs/suite")).is_none());
    let error = log_folder(home.path(), Path::new("plugin-logs/suite")).unwrap_err();
    assert!(error.to_string().ends_with("` is a link or not a folder"), "{error}");
    assert_eq!(std::fs::read_to_string(elsewhere.path().join(STDERR_LOG)).unwrap(), "keep me");
    assert_eq!(std::fs::read_to_string(elsewhere.path().join(OLD_LOG)).unwrap(), "keep me too");
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 2, "nothing made there");
}

#[cfg(unix)]
#[test]
fn a_linked_folder_on_the_way_to_the_log_is_refused() {
    let (home, elsewhere) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(elsewhere.path(), home.path().join("plugin-logs")).unwrap();
    assert!(open(home.path(), Path::new("plugin-logs/suite")).is_none());
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0, "nothing made there");
}
