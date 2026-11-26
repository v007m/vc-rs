//! One test's results over the newest test runs, for the per-test strip (D673).

use vc_interfaces::{RunFilter, RunRow, TestRow};
use vc_model::TestStatus;

use crate::history::history_error;
use crate::load::RUN_KIND;
use crate::{Engine, EngineError};

/// The most runs one strip reads, so a request cannot read the whole history.
pub const MOST_RUNS: usize = 100;

/// How one test did in one run.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TestResults {
    /// The run.
    pub run: RunRow,
    /// The test's instances in it, each example row and repeat, without their exchanges;
    /// empty when the run did not run the test.
    pub instances: Vec<TestRow>,
}

impl TestResults {
    /// The worst way an instance ended: failed, blocked, allowed to fail, flaky, passed, then
    /// skipped; `None` when the run did not run the test.
    #[must_use]
    pub fn status(&self) -> Option<TestStatus> {
        self.instances.iter().filter_map(|test| test.status).max_by_key(|status| severity(*status))
    }

    /// The instances' time together, in milliseconds.
    #[must_use]
    pub fn duration_ms(&self) -> f64 {
        self.instances.iter().map(|test| test.duration_ms).sum()
    }
}

impl Engine {
    /// How test `test` (its full ID) did in each of the newest `limit` test runs, at most
    /// [`MOST_RUNS`], newest first; a run that did not run it has no instances.
    ///
    /// # Errors
    ///
    /// [`EngineError::Selection`] when `test` is empty, and [`EngineError::History`] when the
    /// history cannot be read.
    pub fn test_history(&self, test: &str, limit: usize) -> Result<Vec<TestResults>, EngineError> {
        if test.is_empty() {
            let help = Some("pass a test's full ID, such as `users.crud#listsUsers`".to_owned());
            return Err(EngineError::Selection { reason: "name the test".to_owned(), help });
        }
        let reader = self.history_reader();
        let filter = RunFilter { kind: Some(RUN_KIND.to_owned()), ..RunFilter::default() };
        let runs = reader.runs(limit.min(MOST_RUNS), &filter).map_err(|error| history_error(&error))?;
        runs.into_iter()
            .map(|run| {
                let tests = reader.tests(run.id).map_err(|error| history_error(&error))?;
                let instances = tests.into_iter().filter(|row| row.test_id == test).map(without_exchanges).collect();
                Ok(TestResults { run, instances })
            })
            .collect()
    }
}

fn without_exchanges(test: TestRow) -> TestRow {
    TestRow { exchanges: Vec::new(), ..test }
}

/// How bad `status` is, for [`TestResults::status`].
const fn severity(status: TestStatus) -> u8 {
    match status {
        TestStatus::Skipped => 0,
        TestStatus::Passed => 1,
        TestStatus::Flaky => 2,
        TestStatus::AllowedFailure => 3,
        TestStatus::Blocked => 4,
        TestStatus::Failed => 5,
    }
}

#[cfg(test)]
#[path = "test_history_tests.rs"]
mod tests;
