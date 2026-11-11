use std::collections::BTreeMap;
use std::time::Duration;

use vc_interfaces::{Vcs, VcsError};
use vc_model::SourcePath;

/// A [`Vcs`] that knows the changed files for a fixed set of base refs, and the recent ones.
///
/// Refs are checked as every `Vcs` checks them; a ref it was not given fails as an unknown ref
/// does in git. The recent files are the same whatever the window.
#[derive(Clone, Debug, Default)]
pub struct FakeVcs {
    changes: BTreeMap<String, Vec<SourcePath>>,
    recent: Vec<SourcePath>,
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
        Self { changes, recent: history.recent() }
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
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Vcs;
    use vc_model::SourcePath;

    use super::FakeVcs;
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
}
