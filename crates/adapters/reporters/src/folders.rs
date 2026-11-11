//! The report folders of runs the history no longer keeps: `history.keep` and
//! `vc history prune` drop a run's `.vibranium/reports/<run>/` with its row.

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;

use vc_interfaces::{ReportError, RunId};

/// Removes each folder under `dir` named by the number of a run older than `oldest`, the oldest
/// run the history keeps, or every such folder when it keeps none, and says how many went.
/// Anything else under `dir`, a link among them, stays; a `dir` that does not exist has none.
///
/// # Errors
///
/// [`ReportError::Write`] when `dir` cannot be listed or a folder cannot be removed.
pub fn prune_run_folders(dir: &Path, oldest: Option<RunId>) -> Result<usize, ReportError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(failed(dir, &error)),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if !folder || !pruned(&entry.file_name(), oldest) {
            continue;
        }
        let path = entry.path();
        fs::remove_dir_all(&path).map_err(|error| failed(&path, &error))?;
        removed += 1;
    }
    Ok(removed)
}

/// Whether `name` is a run number, written plainly, of a run older than `oldest`.
fn pruned(name: &OsStr, oldest: Option<RunId>) -> bool {
    let number = name.to_str().and_then(|name| name.parse::<u64>().ok().filter(|number| number.to_string() == name));
    number.is_some_and(|number| oldest.is_none_or(|oldest| number < oldest.get()))
}

fn failed(path: &Path, error: &io::Error) -> ReportError {
    ReportError::Write { location: path.display().to_string(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;

    use vc_interfaces::RunId;

    use super::{prune_run_folders, pruned};

    #[test]
    fn removes_only_the_folders_of_runs_older_than_the_oldest_kept() {
        let root = tempfile::tempdir().unwrap();
        for name in ["1", "2", "3", "4", "007", "notes"] {
            fs::create_dir(root.path().join(name)).unwrap();
            fs::write(root.path().join(name).join("ctrf.json"), "{}").unwrap();
        }
        fs::write(root.path().join("0"), "a file, not a folder").unwrap();
        assert_eq!(prune_run_folders(root.path(), Some(RunId::new(3))), Ok(2));
        let mut left: Vec<String> =
            fs::read_dir(root.path()).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, ["0", "007", "3", "4", "notes"]);
        assert_eq!(prune_run_folders(root.path(), None), Ok(2), "none kept: every run folder goes");
        assert_eq!(prune_run_folders(&root.path().join("missing"), None), Ok(0));
    }

    #[test]
    fn reads_a_run_number_only_when_written_plainly() {
        let oldest = Some(RunId::new(10));
        assert!(pruned(OsStr::new("9"), oldest));
        assert!(!pruned(OsStr::new("10"), oldest));
        assert!(!pruned(OsStr::new("+9"), oldest) && !pruned(OsStr::new("09"), oldest));
        assert!(!pruned(OsStr::new("run-9"), None));
    }
}
