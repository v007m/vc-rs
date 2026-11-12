//! Which stored runs a history query counts (G15): the History tab's kind and Insights' profile
//! and branch, applied by the store so a limit or window is counted among the runs that match.

use crate::RunStart;

/// The runs a [`crate::HistoryReader`] query counts; a field left `None` matches every run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history query built and read across crates; every user is in this repo (D160)"
)]
pub struct RunFilter {
    /// Only runs of this kind: `run`, `load`, `adhoc` or `agent`.
    pub kind: Option<String>,
    /// Only runs with this among their active profiles.
    pub profile: Option<String>,
    /// Only runs on this git branch; a run whose branch is not known is left out.
    pub branch: Option<String>,
}

impl RunFilter {
    /// Whether the run that started as `start` is one the filter counts.
    #[must_use]
    pub fn matches(&self, start: &RunStart) -> bool {
        self.kind.as_ref().is_none_or(|kind| &start.kind == kind)
            && self.profile.as_ref().is_none_or(|profile| start.profiles.contains(profile))
            && self.branch.as_ref().is_none_or(|branch| start.git_branch.as_ref() == Some(branch))
    }
}

#[cfg(test)]
mod tests {
    use super::RunFilter;
    use crate::RunStart;

    fn start(kind: &str, profiles: &[&str], branch: Option<&str>) -> RunStart {
        let profiles = profiles.iter().map(|&profile| profile.to_owned()).collect();
        RunStart { kind: kind.into(), profiles, git_branch: branch.map(str::to_owned), ..RunStart::default() }
    }

    #[test]
    fn a_filter_matches_runs_by_kind_profile_and_branch_and_none_matches_all() {
        let run = start("run", &["staging", "eu"], Some("main"));
        assert!(RunFilter::default().matches(&run));
        let wanted = |kind: &str, profile: &str, branch: &str| RunFilter {
            kind: Some(kind.into()),
            profile: Some(profile.into()),
            branch: Some(branch.into()),
        };
        assert!(wanted("run", "eu", "main").matches(&run), "any of its profiles");
        assert!(!wanted("load", "eu", "main").matches(&run));
        assert!(!wanted("run", "dev", "main").matches(&run));
        assert!(!wanted("run", "eu", "feat/x").matches(&run));
        let on_main = RunFilter { branch: Some("main".into()), ..RunFilter::default() };
        assert!(!on_main.matches(&start("run", &[], None)), "an unknown branch is not `main`");
    }
}
