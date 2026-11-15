//! The report folders of runs the history no longer keeps: `history.keep` and
//! `vc history prune` drop a run's `<reports.dir>/<run>/` with its row. Only a folder `vc` made
//! for a run, and marked as one, is ever removed. [`resolves_inside`] tells whether the
//! project's own report folder stays inside the project on disk, links followed (D327).

use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path};

use vc_interfaces::{ReportError, RunId};

/// The empty file that marks a folder under `reports.dir` as a run's report folder.
pub const RUN_MARKER: &str = ".vc-run";

/// Makes `dir`, a run's report folder, and marks it as one ([`RUN_MARKER`]).
///
/// # Errors
///
/// [`ReportError::Write`] when the folder or its marker cannot be written.
pub fn mark_run_folder(dir: &Path) -> Result<(), ReportError> {
    fs::create_dir_all(dir).map_err(|error| failed(dir, &error))?;
    let marker = dir.join(RUN_MARKER);
    fs::write(&marker, "").map_err(|error| failed(&marker, &error))
}

/// Removes the run folders under `dir` of the runs older than `below`, and says how many went.
///
/// A run folder is named by its run's number and holds [`RUN_MARKER`]. Anything else under
/// `dir` stays: a folder of the user's own, even one named by a number, a link and a file. A
/// `dir` that does not exist has none.
///
/// # Errors
///
/// [`ReportError::Write`] when `dir` cannot be listed or a folder cannot be removed.
pub fn prune_run_folders(dir: &Path, below: RunId) -> Result<usize, ReportError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(failed(dir, &error)),
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let folder = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if !folder || !pruned(&entry.file_name(), below) || !marked(&path) {
            continue;
        }
        fs::remove_dir_all(&path).map_err(|error| failed(&path, &error))?;
        removed += 1;
    }
    Ok(removed)
}

/// Whether `dir` stays inside `root` on disk, links followed.
///
/// The nearest part of it that exists must resolve to `root` or under it, and the parts after
/// that be plain names. A `dir` none of whose parts exists, or one that cannot be resolved (a
/// link to nowhere), does not.
#[must_use]
pub fn resolves_inside(root: &Path, dir: &Path) -> bool {
    let Ok(root) = fs::canonicalize(root) else { return false };
    let Some(existing) = dir.ancestors().find(|at| at.symlink_metadata().is_ok()) else { return false };
    let plain =
        dir.strip_prefix(existing).is_ok_and(|rest| rest.components().all(|part| matches!(part, Component::Normal(_))));
    plain && fs::canonicalize(existing).is_ok_and(|real| real.starts_with(&root))
}

/// Whether `name` is a run number, written plainly, of a run older than `below`.
fn pruned(name: &OsStr, below: RunId) -> bool {
    let number = name.to_str().and_then(|name| name.parse::<u64>().ok().filter(|number| number.to_string() == name));
    number.is_some_and(|number| number < below.get())
}

/// Whether the folder `path` holds the marker of a run's folder: a file, not a link.
fn marked(path: &Path) -> bool {
    fs::symlink_metadata(path.join(RUN_MARKER)).is_ok_and(|marker| marker.is_file())
}

fn failed(path: &Path, error: &io::Error) -> ReportError {
    ReportError::Write { location: path.display().to_string(), reason: error.to_string() }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;

    use vc_interfaces::RunId;

    use super::{RUN_MARKER, mark_run_folder, prune_run_folders, pruned, resolves_inside};

    fn left(root: &Path) -> Vec<String> {
        let mut left: Vec<String> =
            fs::read_dir(root).unwrap().map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        left
    }

    #[test]
    fn removes_only_the_marked_folders_of_runs_older_than_the_oldest_kept() {
        let root = tempfile::tempdir().unwrap();
        for name in ["1", "2", "3", "4", "007", "notes"] {
            mark_run_folder(&root.path().join(name)).unwrap();
            fs::write(root.path().join(name).join("ctrf.json"), "{}").unwrap();
        }
        fs::write(root.path().join("0"), "a file, not a folder").unwrap();
        assert_eq!(prune_run_folders(root.path(), RunId::new(3)), Ok(2));
        assert_eq!(left(root.path()), ["0", "007", "3", "4", "notes"]);
        assert_eq!(prune_run_folders(&root.path().join("missing"), RunId::new(9)), Ok(0));
        assert!(prune_run_folders(&root.path().join("0"), RunId::new(9)).is_err(), "a file is not a folder of runs");
    }

    #[test]
    fn keeps_a_folder_named_by_a_number_that_vc_did_not_mark() {
        let root = tempfile::tempdir().unwrap();
        for name in ["1", "2024"] {
            fs::create_dir(root.path().join(name)).unwrap();
            fs::write(root.path().join(name).join("notes.txt"), "mine").unwrap();
        }
        fs::create_dir_all(root.path().join("5").join(RUN_MARKER)).unwrap();
        assert_eq!(prune_run_folders(root.path(), RunId::new(9000)), Ok(0));
        assert_eq!(left(root.path()), ["1", "2024", "5"]);
        assert!(root.path().join("2024/notes.txt").is_file());
    }

    #[test]
    fn marks_a_run_folder_with_an_empty_marker() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("reports").join("7");
        assert_eq!(mark_run_folder(&dir), Ok(()));
        assert_eq!(fs::read(dir.join(RUN_MARKER)).unwrap(), b"");
        fs::write(root.path().join("taken"), "a file").unwrap();
        assert!(mark_run_folder(&root.path().join("taken")).is_err());
    }

    #[test]
    fn reads_a_run_number_only_when_written_plainly() {
        let below = RunId::new(10);
        assert!(pruned(OsStr::new("9"), below));
        assert!(!pruned(OsStr::new("10"), below));
        assert!(!pruned(OsStr::new("+9"), below) && !pruned(OsStr::new("09"), below));
        assert!(!pruned(OsStr::new("run-9"), below));
    }

    #[test]
    fn a_folder_resolves_inside_until_a_link_leads_it_out() {
        let (root, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let root = root.path();
        assert!(resolves_inside(root, &root.join(".vibranium/reports")), "nothing made yet: the root decides");
        fs::create_dir_all(root.join("reports/ci")).unwrap();
        assert!(resolves_inside(root, &root.join("reports/ci")));
        assert!(!resolves_inside(root, &root.join("reports/../../b")), "`..` resolves above the root");
        assert!(!resolves_inside(root, outside.path()));
        assert!(!resolves_inside(&root.join("missing"), &root.join("reports")), "a root that is not there");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_root_or_to_nowhere_does_not_resolve_inside() {
        let (root, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let root = root.path();
        std::os::unix::fs::symlink(outside.path(), root.join("out")).unwrap();
        assert!(!resolves_inside(root, &root.join("out")));
        assert!(!resolves_inside(root, &root.join("out/reports/1")), "a folder not made yet under the link");
        std::os::unix::fs::symlink(root.join("gone"), root.join("dangling")).unwrap();
        assert!(!resolves_inside(root, &root.join("dangling/reports")));
        fs::create_dir(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("inner")).unwrap();
        assert!(resolves_inside(root, &root.join("inner/reports")), "a link that stays inside is fine");
    }
}
