use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use vc_interfaces::{Vcs, VcsCommit, VcsError, VcsStatus};
use vc_model::SourcePath;

/// A [`Vcs`] that knows the changed files for a fixed set of base refs, and the recent ones;
/// for the git pane, a set status and diffs, and commits, pushes and pulls it records.
///
/// Refs are checked as every `Vcs` checks them; a ref it was not given fails as an unknown ref
/// does in git. The recent files are the same whatever the window. A file it has no diff for
/// has an empty one. A commit gives the ID `abc1234`, a push says `pushed` and a pull `pulled`,
/// unless it was told to fail ([`FakeVcs::failing`]).
#[derive(Clone, Debug, Default)]
pub struct FakeVcs {
    changes: BTreeMap<String, Vec<SourcePath>>,
    recent: Vec<SourcePath>,
    status: VcsStatus,
    diffs: BTreeMap<SourcePath, String>,
    failure: Option<String>,
    asks: Option<Sender<VcsAsk>>,
}

/// A change the git pane asked a [`FakeVcs`] for.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum VcsAsk {
    /// A commit.
    Commit(VcsCommit),
    /// A push.
    Push,
    /// A pull.
    Pull,
}

/// The changes a [`FakeVcs`] was asked for.
#[derive(Debug)]
pub struct VcsAsks(Receiver<VcsAsk>);

impl VcsAsks {
    /// Everything asked since the last call, oldest first.
    #[must_use]
    pub fn take(&self) -> Vec<VcsAsk> {
        self.0.try_iter().collect()
    }
}

impl FakeVcs {
    /// A fake whose files changed since `base` are `paths` (invalid paths are dropped).
    #[must_use]
    pub fn new(base: &str, paths: &[&str]) -> Self {
        Self::default().with(base, paths)
    }

    /// The fake history a [`crate::vcs_suite`] repository has.
    #[must_use]
    pub fn of(history: &crate::VcsHistory) -> Self {
        let changes = BTreeMap::from([(crate::VcsHistory::BASE.to_owned(), history.changed())]);
        Self { changes, recent: history.recent(), ..Self::default() }
    }

    /// Also knows that `paths` changed since `base`.
    #[must_use]
    pub fn with(mut self, base: &str, paths: &[&str]) -> Self {
        self.changes.insert(base.to_owned(), sorted(paths));
        self
    }

    /// Also knows that `paths` changed lately, in any window.
    #[must_use]
    pub fn with_recent(mut self, paths: &[&str]) -> Self {
        self.recent = sorted(paths);
        self
    }

    /// Reports `status` as the git pane's.
    #[must_use]
    pub fn with_status(mut self, status: VcsStatus) -> Self {
        self.status = status;
        self
    }

    /// Gives `text` as the diff of `path`.
    #[must_use]
    pub fn with_diff(mut self, path: &SourcePath, text: &str) -> Self {
        self.diffs.insert(path.clone(), text.to_owned());
        self
    }

    /// Fails every commit, push and pull with `reason`, still recording it.
    #[must_use]
    pub fn failing(mut self, reason: &str) -> Self {
        self.failure = Some(reason.to_owned());
        self
    }

    /// Records each commit, push and pull it is asked for, read from the returned end.
    #[must_use]
    pub fn recording(mut self) -> (Self, VcsAsks) {
        let (asks, inbox) = mpsc::channel();
        self.asks = Some(asks);
        (self, VcsAsks(inbox))
    }

    /// Records `ask`, then `said` unless the fake fails.
    fn asked(&self, (ask, action, said): (VcsAsk, &str, &str)) -> Result<String, VcsError> {
        if let Some(asks) = &self.asks {
            asks.send(ask).ok();
        }
        let refused = |reason: &String| Err(VcsError::Action { action: action.to_owned(), reason: reason.clone() });
        self.failure.as_ref().map_or_else(|| Ok(said.to_owned()), refused)
    }
}

fn sorted(paths: &[&str]) -> Vec<SourcePath> {
    let mut paths: Vec<SourcePath> = paths.iter().filter_map(|path| SourcePath::new(path).ok()).collect();
    paths.sort();
    paths.dedup();
    paths
}

impl Vcs for FakeVcs {
    fn changed(&self, base: &str) -> Result<Vec<SourcePath>, VcsError> {
        let base = vc_interfaces::checked_ref(base)?;
        let unknown = || VcsError::Failed { base: base.to_owned(), reason: "unknown revision".to_owned() };
        self.changes.get(base).cloned().ok_or_else(unknown)
    }

    fn changed_within(&self, _window: Duration) -> Result<Vec<SourcePath>, VcsError> {
        Ok(self.recent.clone())
    }

    fn status(&self) -> Result<VcsStatus, VcsError> {
        Ok(self.status.clone())
    }

    fn diff(&self, path: &SourcePath) -> Result<String, VcsError> {
        Ok(self.diffs.get(path).cloned().unwrap_or_default())
    }

    fn commit(&self, commit: &VcsCommit) -> Result<String, VcsError> {
        self.asked((VcsAsk::Commit(commit.clone()), "commit", "abc1234"))
    }

    fn push(&self) -> Result<String, VcsError> {
        self.asked((VcsAsk::Push, "push", "pushed"))
    }

    fn pull(&self) -> Result<String, VcsError> {
        self.asked((VcsAsk::Pull, "pull", "pulled"))
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Vcs;
    use vc_model::SourcePath;

    use vc_interfaces::{VcsCommit, VcsError, VcsStatus};

    use super::{FakeVcs, VcsAsk};
    use crate::{VcsHistory, vcs_suite};

    #[test]
    fn meets_the_vcs_contract() {
        vcs_suite(&mut |history| Box::new(FakeVcs::of(history)) as Box<dyn Vcs>);
    }

    #[test]
    fn lists_each_bases_paths_sorted_once() {
        let fake = FakeVcs::new("main", &["b.test.yaml", "a.api.yaml", "b.test.yaml", "/abs"]).with("HEAD~1", &[]);
        let paths = |base| fake.changed(base).unwrap().iter().map(SourcePath::to_string).collect::<Vec<_>>();
        assert_eq!(paths("main"), ["a.api.yaml", "b.test.yaml"]);
        assert_eq!(paths("HEAD~1"), Vec::<String>::new());
        assert!(fake.changed(VcsHistory::BASE).is_err());
        assert_eq!(fake.changed_within(std::time::Duration::ZERO), Ok(Vec::new()));
        let recent = fake.with_recent(&["b.test.yaml", "a.test.yaml"]).changed_within(std::time::Duration::ZERO);
        assert_eq!(
            recent.unwrap().iter().map(SourcePath::to_string).collect::<Vec<_>>(),
            ["a.test.yaml", "b.test.yaml"]
        );
    }

    #[test]
    fn gives_its_status_and_diffs_and_records_each_change_asked_for() {
        let path = SourcePath::new("a.yaml").unwrap();
        let status = VcsStatus { branch: "main".to_owned(), ..VcsStatus::default() };
        let (fake, asks) = FakeVcs::default().with_status(status.clone()).with_diff(&path, "+a\n").recording();
        assert_eq!(fake.status(), Ok(status));
        assert_eq!(fake.diff(&path), Ok("+a\n".to_owned()));
        assert_eq!(fake.diff(&SourcePath::new("b.yaml").unwrap()), Ok(String::new()));
        let commit = VcsCommit { paths: vec![path], message: "m".to_owned() };
        assert_eq!(fake.commit(&commit), Ok("abc1234".to_owned()));
        assert_eq!((fake.push(), fake.pull()), (Ok("pushed".to_owned()), Ok("pulled".to_owned())));
        assert_eq!(asks.take(), [VcsAsk::Commit(commit), VcsAsk::Push, VcsAsk::Pull]);
        let refused = fake.failing("denied").push();
        assert_eq!(refused, Err(VcsError::Action { action: "push".to_owned(), reason: "denied".to_owned() }));
        assert_eq!(asks.take(), [VcsAsk::Push]);
    }
}
