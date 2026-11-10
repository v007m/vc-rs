use super::{OLD_LOG, STDERR_LOG, open};

#[test]
fn a_log_before_that_cannot_be_kept_is_removed_and_the_new_log_says_so() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(STDERR_LOG), "the start before").unwrap();
    std::fs::create_dir(dir.path().join(OLD_LOG)).unwrap();
    std::fs::write(dir.path().join(OLD_LOG).join("kept"), "a folder is in the way").unwrap();
    drop(open(dir.path()).unwrap());
    let written = std::fs::read_to_string(dir.path().join(STDERR_LOG)).unwrap();
    assert!(written.starts_with("vibranium: the log before could not be kept as stderr.log.1 ("), "{written}");
    assert!(written.trim_end().ends_with("so it was removed"), "{written}");
}

#[test]
fn the_log_before_is_kept_as_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(STDERR_LOG), "the start before").unwrap();
    drop(open(dir.path()).unwrap());
    assert_eq!(std::fs::read_to_string(dir.path().join(OLD_LOG)).unwrap(), "the start before");
    assert_eq!(std::fs::read_to_string(dir.path().join(STDERR_LOG)).unwrap(), "");
}
