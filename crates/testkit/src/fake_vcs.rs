use std::collections::BTreeMap;

use vc_interfaces::{Vcs, VcsError};
use vc_model::SourcePath;

/// A [`Vcs`] that knows the changed files for a fixed set of base refs.
///
/// Refs are checked as every `Vcs` checks them; a ref it was not given fails as an unknown ref
/// does in git.
#[derive(Clone, Debug, Default)]
pub struct FakeVcs {
    changes: BTreeMap<String, Vec<SourcePath>>,
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
        Self { changes }
    }

    /// Also knows that `paths` changed since `base`.
    #[must_use]
    pub fn with(mut self, base: &str, paths: &[&str]) -> Self {
        let mut paths: Vec<SourcePath> = paths.iter().filter_map(|path| SourcePath::new(path).ok()).collect();
        paths.sort();
        paths.dedup();
        self.changes.insert(base.to_owned(), paths);
        self
    }
}

impl Vcs for FakeVcs {
    fn changed(&self, base: &str) -> Result<Vec<SourcePath>, VcsError> {
        let base = vc_interfaces::checked_ref(base)?;
        let unknown = || VcsError::Failed { base: base.to_owned(), reason: "unknown revision".to_owned() };
        self.changes.get(base).cloned().ok_or_else(unknown)
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
    }
}
