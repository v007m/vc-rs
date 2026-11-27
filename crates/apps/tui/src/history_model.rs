//! What the History tab works out from the stored rows: two runs compared test by test, the
//! tests that keep failing or flaking, and the figures a run's row shows.

use std::cmp::Reverse;
use vc_engine::{RunRow, SLOWER, TestChange, TestRow, TestTrend, Totals, compare_tests};

use crate::theme::clean;

/// The kind the engine stores a load run as.
const LOAD: &str = "load";

/// A test that took longer in the later run.
#[derive(Clone, Debug, PartialEq)]
pub struct Slower {
    /// The test, as `vc history show` names it.
    pub test: String,
    /// Its time in the earlier run, in milliseconds.
    pub before_ms: f64,
    /// Its time in the later run.
    pub after_ms: f64,
}

/// The later of two runs against the earlier, test by test.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comparison {
    /// The earlier run's number.
    pub before: u64,
    /// The later run's number.
    pub after: u64,
    /// Failed in the later run, but not in the earlier (or new in it).
    pub newly_failing: Vec<String>,
    /// Blocked in the later run, but not failing in the earlier.
    pub newly_blocked: Vec<String>,
    /// Failing in the earlier run, passed in the later.
    pub fixed: Vec<String>,
    /// Ran more than 20% longer in the later run.
    pub slower: Vec<Slower>,
    /// Failing in both.
    pub still_failing: Vec<String>,
}

impl Comparison {
    /// `after`'s tests (run `runs.1`) against `before`'s (run `runs.0`), in `after`'s order, as
    /// the engine compares them ([`compare_tests`]).
    pub fn new(runs: (u64, u64), before: &[TestRow], after: &[TestRow]) -> Self {
        let compared = compare_tests(runs, (before, after), SLOWER);
        let labels = |changes: Vec<TestChange>| changes.into_iter().map(|change| clean(&change.label)).collect();
        Self {
            before: compared.before,
            after: compared.after,
            slower: compared.slower.into_iter().map(slower).collect(),
            newly_failing: labels(compared.newly_failing),
            newly_blocked: labels(compared.newly_blocked),
            fixed: labels(compared.fixed),
            still_failing: labels(compared.still_failing),
        }
    }

    /// Whether nothing changed for the worse or the better.
    pub const fn unchanged(&self) -> bool {
        self.newly_failing.is_empty()
            && self.newly_blocked.is_empty()
            && self.fixed.is_empty()
            && self.slower.is_empty()
            && self.still_failing.is_empty()
    }
}

/// A test the engine found slower, fit for the screen.
fn slower(change: TestChange) -> Slower {
    let TestChange { label, before_ms, after_ms, .. } = change;
    Slower { test: clean(&label), before_ms: before_ms.unwrap_or_default(), after_ms }
}

/// The tests that failed, were blocked or were flaky over the trend's runs, the most
/// troubled first, then by ID.
pub fn troubled(trends: &[TestTrend]) -> Vec<&TestTrend> {
    let mut troubled: Vec<&TestTrend> = trends.iter().filter(|trend| trend.flaky + trend.failed > 0).collect();
    troubled.sort_by_key(|trend| Reverse(trend.flaky + trend.failed));
    troubled
}

/// The newest run older than the one at `at` and of its kind: the one it compares with, as a
/// load run's totals and tests are not a test run's. A load run stores no tests, so it compares
/// with none.
pub fn earlier(runs: &[RunRow], at: usize) -> Option<&RunRow> {
    let kind = &runs.get(at).filter(|run| ran_tests(run))?.start.kind;
    runs.iter().skip(at + 1).find(|run| run.start.kind == *kind)
}

/// Whether `run` ran tests, so its pass rate and time belong in the trends: a load run did not.
pub fn ran_tests(run: &RunRow) -> bool {
    run.start.kind != LOAD
}

/// The tests that ran and passed, flaky ones included, in tenths of a percent: `961` for
/// 96.1%; `None` when none ran. An allowed failure ran but did not pass, so a run of nothing
/// else shows 0%.
pub fn pass_rate(totals: &Totals) -> Option<usize> {
    let passed = totals.passed + totals.flaky;
    let ran = passed + totals.failed + totals.blocked + totals.allowed_failures;
    (ran > 0).then(|| passed * 1_000 / ran)
}

/// `96.1%`, or `–` when no test ran.
pub fn percent(totals: &Totals) -> String {
    pass_rate(totals).map_or_else(|| "–".to_owned(), |rate| format!("{}.{}%", rate / 10, rate % 10))
}

/// `load · 40 iterations · 3 errors`, as `vc history` lists a load run: its totals count
/// iterations, the failed ones as errors.
pub fn iterations(totals: &Totals) -> String {
    format!("load · {} iterations · {} errors", totals.passed.saturating_add(totals.failed), totals.failed)
}

/// How long a run took, in milliseconds; `None` until it ends.
pub fn took(run: &RunRow) -> Option<u64> {
    run.finished_ms.map(|finished| finished.saturating_sub(run.start.started_ms))
}

/// `48.2s`, or `350ms` under a second.
pub fn seconds(ms: u64) -> String {
    if ms < 1_000 { format!("{ms}ms") } else { format!("{}.{}s", ms / 1_000, ms % 1_000 / 100) }
}

/// `1.8s` or `350ms` for a mean or a test's time.
pub fn mean(ms: f64) -> String {
    if ms < 1_000.0 { format!("{ms:.0}ms") } else { format!("{:.1}s", ms / 1_000.0) }
}

/// `main@a1c9e4f`: the branch and the commit's first seven characters, either alone, or `–`.
pub fn checkout(run: &RunRow) -> String {
    let commit = run.start.git_commit.as_deref().map(|commit| commit.chars().take(7).collect::<String>());
    let text = match (run.start.git_branch.as_deref(), commit) {
        (Some(branch), Some(commit)) => format!("{branch}@{commit}"),
        (Some(branch), None) => branch.to_owned(),
        (None, Some(commit)) => format!("@{commit}"),
        (None, None) => "–".to_owned(),
    };
    clean(&text)
}

#[cfg(test)]
#[path = "history_model_tests.rs"]
mod tests;
