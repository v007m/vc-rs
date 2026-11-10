//! What the History tab works out from the stored rows: two runs compared test by test, the
//! tests that keep failing or flaking, and the figures a run's row shows.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt::Write as _;

use vc_engine::{RunRow, TestRow, TestStatus, TestTrend, Totals};

use crate::run_screen::failing;
use crate::theme::clean;

/// A test is slower when its time grows by more than this factor (20%).
const SLOWER: f64 = 1.2;
/// The kind the engine stores a load run as.
const LOAD: &str = "load";

/// One test instance across runs: its ID, `examples` row and repeat.
type Key<'a> = (&'a str, Option<u32>, u32);

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
    /// `after`'s tests (run `runs.1`) against `before`'s (run `runs.0`), in `after`'s order.
    pub fn new(runs: (u64, u64), before: &[TestRow], after: &[TestRow]) -> Self {
        let earlier: HashMap<Key<'_>, &TestRow> = before.iter().map(|test| (key(test), test)).collect();
        let mut comparison = Self { before: runs.0, after: runs.1, ..Self::default() };
        for test in after {
            comparison.add(earlier.get(&key(test)).copied(), test);
        }
        comparison
    }

    /// Whether nothing changed for the worse or the better.
    pub const fn unchanged(&self) -> bool {
        self.newly_failing.is_empty()
            && self.newly_blocked.is_empty()
            && self.fixed.is_empty()
            && self.slower.is_empty()
            && self.still_failing.is_empty()
    }

    fn add(&mut self, before: Option<&TestRow>, after: &TestRow) {
        let label = label(after);
        if let Some(before) = before.filter(|before| slower(before, after)) {
            let (before_ms, after_ms) = (before.duration_ms, after.duration_ms);
            self.slower.push(Slower { test: label.clone(), before_ms, after_ms });
        }
        let was = before.and_then(|before| before.status).is_some_and(failing);
        match (was, after.status) {
            (false, Some(TestStatus::Failed)) => self.newly_failing.push(label),
            (false, Some(TestStatus::Blocked)) => self.newly_blocked.push(label),
            (true, Some(TestStatus::Passed | TestStatus::Flaky)) => self.fixed.push(label),
            (true, Some(TestStatus::Failed | TestStatus::Blocked)) => self.still_failing.push(label),
            _ => {}
        }
    }
}

const fn key(test: &TestRow) -> Key<'_> {
    (test.test_id.as_str(), test.example_row, test.repeat_index)
}

/// Whether `after` ran more than 20% longer than `before`; a test that did not run is never
/// slower.
fn slower(before: &TestRow, after: &TestRow) -> bool {
    let ran = |test: &TestRow| test.status.is_some_and(|status| status != TestStatus::Skipped);
    ran(before) && ran(after) && before.duration_ms > 0.0 && after.duration_ms > before.duration_ms * SLOWER
}

/// A test instance as `vc history show` names it: `users.crud#listsUsers [row 2] (repeat 3)`,
/// both from 1.
pub fn label(test: &TestRow) -> String {
    let mut label = clean(&test.test_id);
    if let Some(row) = test.example_row {
        let _ = write!(label, " [row {}]", row.saturating_add(1));
    }
    if test.repeat_index > 0 {
        let _ = write!(label, " (repeat {})", test.repeat_index.saturating_add(1));
    }
    label
}

/// The tests that failed, were blocked or were flaky over the trend's runs, the most
/// troubled first, then by ID.
pub fn troubled(trends: &[TestTrend]) -> Vec<&TestTrend> {
    let mut troubled: Vec<&TestTrend> = trends.iter().filter(|trend| trend.flaky + trend.failed > 0).collect();
    troubled.sort_by_key(|trend| Reverse(trend.flaky + trend.failed));
    troubled
}

/// The newest run older than the one at `at` and of its kind: the one it compares with, as a
/// load run's totals and tests are not a test run's.
pub fn earlier(runs: &[RunRow], at: usize) -> Option<&RunRow> {
    let kind = &runs.get(at)?.start.kind;
    runs.iter().skip(at + 1).find(|run| run.start.kind == *kind)
}

/// Whether `run` ran tests, so its pass rate and time belong in the trends: a load run did not.
pub fn ran_tests(run: &RunRow) -> bool {
    run.start.kind != LOAD
}

/// The tests that ran and passed, flaky ones included, in tenths of a percent: `961` for
/// 96.1%; `None` when none ran.
pub fn pass_rate(totals: &Totals) -> Option<usize> {
    let passed = totals.passed + totals.flaky;
    let ran = passed + totals.failed + totals.blocked;
    (ran > 0).then(|| passed * 1_000 / ran)
}

/// `96.1%`, or `–` when no test ran.
pub fn percent(totals: &Totals) -> String {
    pass_rate(totals).map_or_else(|| "–".to_owned(), |rate| format!("{}.{}%", rate / 10, rate % 10))
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
