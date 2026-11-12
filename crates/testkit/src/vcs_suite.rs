use std::time::Duration;

use vc_interfaces::{Vcs, VcsError};
use vc_model::SourcePath;

/// A repository's history, as [`vcs_suite`] builds it: what was committed long ago, what is
/// committed at [`VcsHistory::BASE`], then what changes after it. Each file is a project-relative
/// path and its text.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct VcsHistory {
    /// Committed first, dated [`VcsHistory::OLD_DATE`], and not touched since.
    pub old: &'static [(&'static str, &'static str)],
    /// Committed and tagged [`VcsHistory::BASE`], `.gitignore` included.
    pub base: &'static [(&'static str, &'static str)],
    /// Committed after the base.
    pub later: &'static [(&'static str, &'static str)],
    /// Changed in the working tree only, not committed.
    pub edited: &'static [(&'static str, &'static str)],
    /// New files, never added.
    pub untracked: &'static [(&'static str, &'static str)],
    /// New files that `.gitignore` matches.
    pub ignored: &'static [(&'static str, &'static str)],
}

impl VcsHistory {
    /// The tag of the base commit.
    pub const BASE: &'static str = "vc-base";
    /// The commit date of the [`VcsHistory::old`] files, years before any recent window.
    pub const OLD_DATE: &'static str = "2020-01-01T00:00:00Z";

    /// The history every [`Vcs`] is checked against.
    pub const STANDARD: Self = Self {
        old: &[("services/legacy/legacy.api.yaml", "apis: []\n")],
        base: &[
            (".gitignore", "*.log\n"),
            ("vibranium.config.yaml", "syntaxVersion: 1\n"),
            ("services/users/users.api.yaml", "apis: []\n"),
            ("services/users/crud.test.yaml", "tests: []\n"),
            ("services/orders/orders.api.yaml", "apis: []\n"),
        ],
        later: &[("services/orders/orders.api.yaml", "apis: [{ id: list }]\n")],
        edited: &[("services/users/crud.test.yaml", "tests: [{ id: listsUsers }]\n")],
        untracked: &[("services/users/admin.test.yaml", "tests: []\n")],
        ignored: &[("run.log", "noise\n")],
    };

    /// What `changed(BASE)` lists: the later, edited and untracked files, sorted.
    #[must_use]
    pub fn changed(&self) -> Vec<SourcePath> {
        sorted(self.later.iter().chain(self.edited).chain(self.untracked))
    }

    /// What `changed_within` a day lists: every file but the old and the ignored ones, sorted.
    #[must_use]
    pub fn recent(&self) -> Vec<SourcePath> {
        sorted(self.base.iter().chain(self.later).chain(self.edited).chain(self.untracked))
    }
}

fn sorted<'a>(files: impl Iterator<Item = &'a (&'static str, &'static str)>) -> Vec<SourcePath> {
    let mut paths: Vec<SourcePath> = files.filter_map(|(path, _)| SourcePath::new(path).ok()).collect();
    paths.sort();
    paths.dedup();
    paths
}

/// Builds a [`Vcs`] over a repository with this history.
pub type MakeVcs<'a> = dyn FnMut(&VcsHistory) -> Box<dyn Vcs> + 'a;

/// The contract every [`Vcs`] meets (D112). `make` builds one over a repository holding
/// [`VcsHistory::STANDARD`].
///
/// # Panics
///
/// When the implementation breaks the contract.
pub fn vcs_suite(make: &mut MakeVcs<'_>) {
    let history = VcsHistory::STANDARD;
    let vcs = make(&history);
    assert_eq!(vcs.changed(VcsHistory::BASE), Ok(history.changed()), "later, edited and untracked, not ignored");
    for bad in ["", "-p", "--output=x", "a..b", "a b", "$(id)", "HEAD:x"] {
        assert_eq!(vcs.changed(bad), Err(VcsError::InvalidRef { base: bad.to_owned() }), "{bad:?}");
    }
    let missing = vcs.changed("no-such-ref");
    assert!(matches!(missing, Err(VcsError::Failed { ref base, .. }) if base == "no-such-ref"), "{missing:?}");
    let day = Duration::from_hours(24);
    assert_eq!(vcs.changed_within(day), Ok(history.recent()), "committed lately, edited and untracked, not old");
}
