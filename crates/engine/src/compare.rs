//! Two stored runs compared test by test, for the Runs space's compare view, `CompareRuns`
//! and the TUI's History tab (D672).

use std::collections::HashMap;

use vc_interfaces::{ExchangeRow, RunRow, TestRow};
use vc_model::TestStatus;

use crate::body_shape::{ShapeChange, shape_changes};
use crate::load::LOAD_KIND;
use crate::stored::{StoredRun, instance_label};
use crate::{Engine, EngineError};

/// How many times longer a test must take to count as slower, and shorter to count as faster:
/// 20%.
pub const SLOWER: f64 = 1.2;

/// One test instance across runs: its ID, `examples` row and repeat.
type Key<'a> = (&'a str, Option<u32>, u32);

/// One test instance of the later run, with how it did in the earlier.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TestChange {
    /// The test's full ID.
    pub test_id: String,
    /// The instance as `vc history show` names it ([`instance_label`]).
    pub label: String,
    /// How it ended in the earlier run; `None` when it was not in it.
    pub before: Option<TestStatus>,
    /// How it ended in the later run.
    pub after: Option<TestStatus>,
    /// Its time in the earlier run, in milliseconds, when it was in it.
    pub before_ms: Option<f64>,
    /// Its time in the later run.
    pub after_ms: f64,
}

/// The later of two runs against the earlier, test instance by test instance, in the later
/// run's order.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct RunComparison {
    /// The earlier run's number.
    pub before: u64,
    /// The later run's number.
    pub after: u64,
    /// Failed in the later run, and not failed or blocked in the earlier (or not in it).
    pub newly_failing: Vec<TestChange>,
    /// Blocked in the later run, and not failed or blocked in the earlier.
    pub newly_blocked: Vec<TestChange>,
    /// Failed or blocked in the earlier run, passed (or flaky) in the later.
    pub fixed: Vec<TestChange>,
    /// Failed or blocked in both.
    pub still_failing: Vec<TestChange>,
    /// Ran in both, and took longer than the threshold allows in the later.
    pub slower: Vec<TestChange>,
    /// Ran in both, and took less than the earlier time over the threshold in the later.
    pub faster: Vec<TestChange>,
    /// The JSON response shapes that changed, per api and status, from the bodies the history
    /// keeps (`history.bodies`); empty when it keeps none.
    pub shapes: Vec<ShapeChange>,
}

impl RunComparison {
    /// Whether no test changed status or time.
    #[must_use]
    pub const fn unchanged(&self) -> bool {
        self.newly_failing.is_empty()
            && self.newly_blocked.is_empty()
            && self.fixed.is_empty()
            && self.still_failing.is_empty()
            && self.slower.is_empty()
            && self.faster.is_empty()
    }

    fn add(&mut self, before: Option<&TestRow>, after: &TestRow, slower: f64) {
        let change = TestChange {
            test_id: after.test_id.clone(),
            label: instance_label(after),
            before: before.and_then(|before| before.status),
            after: after.status,
            before_ms: before.map(|before| before.duration_ms),
            after_ms: after.duration_ms,
        };
        if let Some(before) = before.filter(|before| ran(before) && ran(after) && before.duration_ms > 0.0) {
            if after.duration_ms > before.duration_ms * slower {
                self.slower.push(change.clone());
            } else if after.duration_ms * slower < before.duration_ms {
                self.faster.push(change.clone());
            }
        }
        let was = change.before.is_some_and(failing);
        match (was, after.status) {
            (false, Some(TestStatus::Failed)) => self.newly_failing.push(change),
            (false, Some(TestStatus::Blocked)) => self.newly_blocked.push(change),
            (true, Some(TestStatus::Passed | TestStatus::Flaky)) => self.fixed.push(change),
            (true, Some(TestStatus::Failed | TestStatus::Blocked)) => self.still_failing.push(change),
            _ => {}
        }
    }
}

/// `after`'s tests (run `runs.1`) against `before`'s (run `runs.0`), a test slower or faster
/// past the factor `slower` ([`SLOWER`] for the usual 20%). No response shapes: those come
/// with [`Engine::compare_runs`].
#[must_use]
pub fn compare_tests(runs: (u64, u64), (before, after): (&[TestRow], &[TestRow]), slower: f64) -> RunComparison {
    let earlier: HashMap<Key<'_>, &TestRow> = before.iter().map(|test| (key(test), test)).collect();
    let mut comparison = RunComparison { before: runs.0, after: runs.1, ..RunComparison::default() };
    for test in after {
        comparison.add(earlier.get(&key(test)).copied(), test, slower);
    }
    comparison
}

impl Engine {
    /// Stored run `runs.1` against the earlier `runs.0` (each `last` or a number), a test
    /// slower or faster past the factor `slower`, with the response shapes that changed in the
    /// bodies their report snapshots kept.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] when `slower` is not a finite factor of at least 1, and
    /// [`EngineError::History`] when a run is not stored, is a load run, or the history
    /// cannot be read.
    pub fn compare_runs(&self, runs: (&str, &str), slower: f64) -> Result<RunComparison, EngineError> {
        if !(slower.is_finite() && slower >= 1.0) {
            return Err(EngineError::Setting {
                origin: "slower_threshold".to_owned(),
                reason: "is not a share of the earlier time of 0 or more".to_owned(),
                help: Some("pass 0.2 for 20%, or 0 for the usual 20%".to_owned()),
            });
        }
        let (before, after) = (tested(self.stored(runs.0)?)?, tested(self.stored(runs.1)?)?);
        let numbers = (before.run().id.get(), after.run().id.get());
        let mut comparison = compare_tests(numbers, (before.tests(), after.tests()), slower);
        comparison.shapes = shape_changes((&self.exchanges(before.run()), &self.exchanges(after.run())));
        Ok(comparison)
    }

    /// The exchanges kept in `run`'s report snapshot; none when it closed without one.
    fn exchanges(&self, run: &RunRow) -> Vec<ExchangeRow> {
        self.snapshot(run).map(|snapshot| snapshot.exchanges()).unwrap_or_default()
    }
}

/// `run`, when it ran tests: a load run stores iterations, not tests.
fn tested(run: StoredRun) -> Result<StoredRun, EngineError> {
    if run.run().start.kind != LOAD_KIND {
        return Ok(run);
    }
    Err(EngineError::History {
        reason: format!("run {} is a load run, which has no tests to compare", run.run().id),
        help: Some("compare two test runs; `vc history --kind run` lists them".to_owned()),
    })
}

const fn key(test: &TestRow) -> Key<'_> {
    (test.test_id.as_str(), test.example_row, test.repeat_index)
}

/// Whether `status` is a failure that fails the run: failed or blocked.
#[must_use]
const fn failing(status: TestStatus) -> bool {
    matches!(status, TestStatus::Failed | TestStatus::Blocked)
}

/// Whether `test` ran: a skipped one did not, so its time says nothing.
fn ran(test: &TestRow) -> bool {
    test.status.is_some_and(|status| status != TestStatus::Skipped)
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
