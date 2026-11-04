use vc_interfaces::{Vcs, VcsError};
use vc_model::SourcePath;

/// A repository's history, as [`vcs_suite`] builds it: what is committed at [`VcsHistory::BASE`],
/// then what changes after it. Each file is a project-relative path and its text.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct VcsHistory {
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

    /// The history every [`Vcs`] is checked against.
    pub const STANDARD: Self = Self {
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
        let paths = self.later.iter().chain(self.edited).chain(self.untracked);
        let mut paths: Vec<SourcePath> = paths.filter_map(|(path, _)| SourcePath::new(path).ok()).collect();
        paths.sort();
        paths.dedup();
        paths
    }
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
}
