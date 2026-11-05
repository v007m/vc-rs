//! `vc run --failed` (D170): the failed and blocked tests of the last run, again, under the
//! same profiles.

use std::collections::{BTreeSet, HashMap};

use vc_interfaces::TestRow;
use vc_model::{Program, Test, TestStatus};

use crate::history::{self, history_error};
use crate::{Engine, EngineError, RunRequest};

impl Engine {
    /// What `request` plans: its own selection, or with `--failed` the last run's failed and
    /// blocked results to select, under the last run's profiles unless it names some.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when there is no last run or the history cannot be read, and
    /// [`EngineError::Selection`] when the last run had no failed or blocked test.
    pub(crate) fn rerun(&self, request: &RunRequest) -> Result<(crate::PlanRequest, Vec<TestRow>), EngineError> {
        let mut plan = request.plan.clone();
        if !request.failed {
            return Ok((plan, Vec::new()));
        }
        if !history::selector(&plan.select).is_empty() {
            return Err(EngineError::Selection {
                reason: "`--failed` picks the tests itself and takes no selector".to_owned(),
                help: Some("drop the selectors, or run them without `--failed`".to_owned()),
            });
        }
        let last = self.stored_run("last")?;
        let tests = self.history_reader().tests(last.id).map_err(|error| history_error(&error))?;
        let failed: Vec<TestRow> = tests.into_iter().filter(|test| needs_rerun(test.status)).collect();
        if failed.is_empty() {
            return Err(EngineError::Selection {
                reason: format!("run {} had no failed or blocked test; nothing to run", last.id),
                help: Some("`vc run` without `--failed` runs every test".to_owned()),
            });
        }
        if plan.profiles.is_empty() {
            plan.profiles = last.start.profiles;
        }
        Ok((plan, failed))
    }
}

const fn needs_rerun(status: Option<TestStatus>) -> bool {
    matches!(status, Some(TestStatus::Failed | TestStatus::Blocked))
}

/// The selectors of `failed` in `program`: a test's ID, or for an `examples` row each of its
/// instances (`users.crud#rejectsBadEmail:3`), numbered as the planner numbers them with
/// `repeat` or the test's own. Tests no longer in the project are left out.
///
/// # Errors
///
/// [`EngineError::Selection`] when none of them is in the project any more.
pub fn selectors(program: &Program, failed: &[TestRow], repeat: Option<u32>) -> Result<Vec<String>, EngineError> {
    let tests: HashMap<&str, &Test> =
        program.tests.values().filter_map(|test| Some((program.str(test.id.sym())?, test))).collect();
    let mut selectors = BTreeSet::new();
    for row in failed {
        let Some(test) = tests.get(row.test_id.as_str()) else { continue };
        let Some(example) = row.example_row else {
            selectors.insert(row.test_id.clone());
            continue;
        };
        let per_row = repeat.unwrap_or(test.repeat).max(1);
        let first = example.saturating_mul(per_row);
        selectors.extend((1..=per_row).map(|n| format!("{}:{}", row.test_id, first.saturating_add(n))));
    }
    if selectors.is_empty() {
        return Err(EngineError::Selection {
            reason: "none of the last run's failed tests is in the project any more".to_owned(),
            help: Some("`vc run` without `--failed` runs every test".to_owned()),
        });
    }
    Ok(selectors.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_interfaces::{HistoryWriter as _, Row, RunStart, TestRow};
    use vc_model::TestStatus;
    use vc_testkit::{FakeVcs, MemoryHistory};

    use super::selectors;
    use crate::{Engine, PlanRequest, RunRequest, sample};

    fn row(id: &str, example_row: Option<u32>, status: TestStatus) -> TestRow {
        TestRow { test_id: id.to_owned(), example_row, status: Some(status), ..TestRow::default() }
    }

    /// The sample engine over a history holding one run under `staging` with `rows`.
    fn engine(rows: Vec<TestRow>) -> Engine {
        let history = Arc::new(MemoryHistory::new());
        if !rows.is_empty() {
            let start = RunStart { profiles: vec!["staging".into()], ..RunStart::default() };
            let run = history.begin(&start).unwrap();
            history.record(run, rows.into_iter().map(Row::Test).collect()).unwrap();
        }
        sample::builder(FakeVcs::default()).history(history.clone(), history).build().unwrap()
    }

    fn failed() -> RunRequest {
        RunRequest { failed: true, ..RunRequest::default() }
    }

    #[test]
    fn reruns_the_failed_and_blocked_tests_under_the_last_profiles() {
        let rows = vec![
            row("users.crud#listsUsers", None, TestStatus::Failed),
            row("orders.flow#listsOrders", None, TestStatus::Passed),
            row("users.crud#skipsUsers", None, TestStatus::Blocked),
        ];
        let (plan, rerun) = engine(rows).rerun(&failed()).unwrap();
        assert_eq!(plan.profiles, ["staging"]);
        let ids: Vec<&str> = rerun.iter().map(|row| row.test_id.as_str()).collect();
        assert_eq!(ids, ["users.crud#listsUsers", "users.crud#skipsUsers"]);
        let plan = PlanRequest { profiles: vec!["dev".into()], ..PlanRequest::default() };
        let named = RunRequest { plan, ..failed() };
        let engine = engine(vec![row("users.crud#listsUsers", None, TestStatus::Failed)]);
        assert_eq!(engine.rerun(&named).unwrap().0.profiles, ["dev"]);
    }

    #[test]
    fn refuses_without_a_run_or_a_failure() {
        let mut selected = failed();
        selected.plan.select.ids.push("users.crud".into());
        assert!(engine(Vec::new()).rerun(&selected).unwrap_err().to_string().contains("takes no selector"));
        assert_eq!(engine(Vec::new()).rerun(&failed()).unwrap_err().to_string(), "history: no run is stored yet");
        let error = engine(vec![row("users.crud#listsUsers", None, TestStatus::Passed)]).rerun(&failed()).unwrap_err();
        assert!(error.to_string().contains("no failed or blocked test"), "{error}");
        let (_, rerun) = engine(Vec::new()).rerun(&RunRequest::default()).unwrap();
        assert!(rerun.is_empty(), "without `--failed` the history is not read");
    }

    #[test]
    fn selects_whole_tests_and_every_instance_of_a_failed_row() {
        let (program, _) = engine(Vec::new()).program().unwrap();
        let failed = [
            row("orders.flow#listsOrders", None, TestStatus::Failed),
            row("users.crud#listsUsers", Some(1), TestStatus::Failed),
            row("users.crud#listsUsers", Some(1), TestStatus::Blocked),
            row("users.crud#gone", None, TestStatus::Failed),
        ];
        let picked = selectors(&program, &failed, None).unwrap();
        assert_eq!(picked, ["orders.flow#listsOrders", "users.crud#listsUsers:3", "users.crud#listsUsers:4"]);
        assert_eq!(selectors(&program, &failed[1..2], Some(1)).unwrap(), ["users.crud#listsUsers:2"]);
        let error = selectors(&program, &failed[3..], None).unwrap_err();
        assert!(error.to_string().contains("any more"), "{error}");
    }
}
