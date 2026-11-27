use std::sync::Arc;

use vc_interfaces::{ExchangeRow, HistoryWriter as _, Row, RunEnd, RunStart, TestRow};
use vc_model::{RunStatus, TestStatus, Totals};
use vc_testkit::{FakeVcs, MemoryHistory};

use crate::{Engine, EngineError, MOST_RUNS, sample};

fn row(test_id: &str, example_row: Option<u32>, status: TestStatus) -> TestRow {
    let exchange = ExchangeRow { api_id: Some("users#list".to_owned()), ..ExchangeRow::default() };
    TestRow {
        test_id: test_id.to_owned(),
        example_row,
        status: Some(status),
        duration_ms: 10.0,
        exchanges: vec![exchange],
        ..TestRow::default()
    }
}

/// A history of `runs`, oldest first, each a kind and its tests.
fn engine(runs: Vec<(&str, Vec<TestRow>)>) -> Engine {
    let history = Arc::new(MemoryHistory::new());
    for (kind, rows) in runs {
        let run = history.begin(&RunStart { kind: kind.to_owned(), ..RunStart::default() }).unwrap();
        history.record(run, rows.into_iter().map(Row::Test).collect()).unwrap();
        let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
        history.finish(run, end).unwrap();
    }
    sample::builder(FakeVcs::default()).history(history.clone(), history).build().unwrap()
}

#[test]
fn gives_the_test_in_each_of_the_newest_test_runs_newest_first() {
    let test = "users.crud#rejectsBadEmail";
    let engine = engine(vec![
        ("run", vec![row(test, Some(0), TestStatus::Passed), row(test, Some(1), TestStatus::Failed)]),
        ("run", vec![row("users.crud#listsUsers", None, TestStatus::Passed)]),
        ("load", Vec::new()),
        ("run", vec![row(test, Some(0), TestStatus::Flaky), row(test, Some(1), TestStatus::Passed)]),
    ]);
    let strip = engine.test_history(test, 10).unwrap();
    let runs: Vec<u64> = strip.iter().map(|results| results.run.id.get()).collect();
    assert_eq!(runs, [4, 2, 1], "test runs only, newest first");
    let statuses: Vec<_> = strip.iter().map(super::TestResults::status).collect();
    assert_eq!(
        statuses,
        [Some(TestStatus::Flaky), None, Some(TestStatus::Failed)],
        "the worst instance; none when not run"
    );
    assert_eq!((strip[0].instances.len(), strip[0].duration_ms()), (2, 20.0));
    assert!(strip.iter().flat_map(|results| &results.instances).all(|test| test.exchanges.is_empty()));
    assert_eq!(engine.test_history(test, 1).unwrap().len(), 1);
}

#[test]
fn reads_at_most_the_cap_and_refuses_an_empty_test() {
    let engine = engine((0..MOST_RUNS + 2).map(|_| ("run", Vec::new())).collect());
    assert_eq!(engine.test_history("a#b", usize::MAX).unwrap().len(), MOST_RUNS);
    assert!(matches!(engine.test_history("", 10), Err(EngineError::Selection { .. })));
}

#[test]
fn ranks_how_an_instance_ended_from_skipped_to_failed() {
    let order = [
        TestStatus::Skipped,
        TestStatus::Passed,
        TestStatus::Flaky,
        TestStatus::AllowedFailure,
        TestStatus::Blocked,
        TestStatus::Failed,
    ];
    for pair in order.windows(2) {
        let results = super::TestResults {
            run: engine(vec![("run", Vec::new())]).history(1, &vc_interfaces::RunFilter::default()).unwrap().runs()[0]
                .clone(),
            instances: pair.iter().map(|status| row("a#b", None, *status)).collect(),
        };
        assert_eq!(results.status(), Some(pair[1]), "{pair:?}");
    }
}
