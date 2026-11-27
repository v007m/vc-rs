use std::sync::Arc;

use serde_json::{Value, json};
use vc_adapter_reporters::sample_snapshot;
use vc_interfaces::{HistoryWriter as _, Row, RunEnd, RunStart, TestRow};
use vc_model::{RunStatus, TestStatus, Totals};
use vc_testkit::{FakeVcs, MemoryHistory};

use super::{RunComparison, SLOWER, compare_tests};
use crate::{Engine, EngineError, sample};

/// The test of [`sample_snapshot`] that fails, with an exchange.
const FAILED_AT: usize = 9;

fn row(test_id: &str, status: TestStatus, duration_ms: f64) -> TestRow {
    TestRow { test_id: test_id.to_owned(), status: Some(status), duration_ms, ..TestRow::default() }
}

fn ids(changes: &[super::TestChange]) -> Vec<&str> {
    changes.iter().map(|change| change.label.as_str()).collect()
}

#[test]
fn sorts_each_test_of_the_later_run_by_how_it_changed() {
    let before = [
        row("a#fails", TestStatus::Passed, 10.0),
        row("a#mends", TestStatus::Failed, 10.0),
        row("a#staysRed", TestStatus::Blocked, 10.0),
        row("a#blocks", TestStatus::Passed, 10.0),
        row("a#flakesBack", TestStatus::Failed, 10.0),
    ];
    let after = [
        row("a#fails", TestStatus::Failed, 10.0),
        row("a#mends", TestStatus::Passed, 10.0),
        row("a#staysRed", TestStatus::Failed, 10.0),
        row("a#blocks", TestStatus::Blocked, 10.0),
        row("a#flakesBack", TestStatus::Flaky, 10.0),
        row("a#isNew", TestStatus::Failed, 10.0),
        row("a#isNewAndPasses", TestStatus::Passed, 10.0),
    ];
    let comparison = compare_tests((3, 4), (&before, &after), SLOWER);
    assert_eq!((comparison.before, comparison.after), (3, 4));
    assert_eq!(ids(&comparison.newly_failing), ["a#fails", "a#isNew"]);
    assert_eq!(ids(&comparison.newly_blocked), ["a#blocks"]);
    assert_eq!(ids(&comparison.fixed), ["a#mends", "a#flakesBack"]);
    assert_eq!(ids(&comparison.still_failing), ["a#staysRed"]);
    let new = &comparison.newly_failing[1];
    assert_eq!((new.before, new.before_ms, new.after), (None, None, Some(TestStatus::Failed)));
}

#[test]
fn a_test_is_slower_or_faster_only_past_the_threshold_and_only_when_it_ran_in_both() {
    let before = [
        row("a#slows", TestStatus::Passed, 100.0),
        row("a#speeds", TestStatus::Passed, 100.0),
        row("a#drifts", TestStatus::Passed, 100.0),
        row("a#wasSkipped", TestStatus::Skipped, 0.0),
        row("a#isSkipped", TestStatus::Passed, 100.0),
    ];
    let after = [
        row("a#slows", TestStatus::Passed, 130.0),
        row("a#speeds", TestStatus::Passed, 70.0),
        row("a#drifts", TestStatus::Passed, 115.0),
        row("a#wasSkipped", TestStatus::Passed, 500.0),
        row("a#isSkipped", TestStatus::Skipped, 0.0),
    ];
    let usual = compare_tests((1, 2), (&before, &after), SLOWER);
    assert_eq!((ids(&usual.slower), ids(&usual.faster)), (vec!["a#slows"], vec!["a#speeds"]));
    assert_eq!((usual.slower[0].before_ms, usual.slower[0].after_ms), (Some(100.0), 130.0));
    let tight = compare_tests((1, 2), (&before, &after), 1.1);
    assert_eq!(ids(&tight.slower), ["a#slows", "a#drifts"]);
    let loose = compare_tests((1, 2), (&before, &after), 2.0);
    assert!(loose.unchanged(), "{loose:?}");
}

#[test]
fn matches_instances_by_example_row_and_repeat() {
    let instance =
        |example_row, repeat_index, status| TestRow { example_row, repeat_index, ..row("a#rows", status, 1.0) };
    let before = [instance(Some(0), 0, TestStatus::Passed), instance(Some(1), 0, TestStatus::Failed)];
    let after = [instance(Some(0), 0, TestStatus::Failed), instance(Some(1), 0, TestStatus::Passed)];
    let comparison = compare_tests((1, 2), (&before, &after), SLOWER);
    assert_eq!(ids(&comparison.newly_failing), ["a#rows [row 1]"]);
    assert_eq!(ids(&comparison.fixed), ["a#rows [row 2]"]);
}

/// A history with run 1 and run 2 holding `tests.0` and `tests.1`, each with the report
/// snapshot `snapshots` gives it, then a load run 3.
fn engine(tests: (Vec<TestRow>, Vec<TestRow>), snapshots: (Vec<u8>, Vec<u8>)) -> Engine {
    let history = Arc::new(MemoryHistory::new());
    for (kind, rows, snapshot) in
        [("run", tests.0, snapshots.0), ("run", tests.1, snapshots.1), ("load", Vec::new(), Vec::new())]
    {
        let run = history.begin(&RunStart { kind: kind.to_owned(), ..RunStart::default() }).unwrap();
        history.record(run, rows.into_iter().map(Row::Test).collect()).unwrap();
        let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot };
        history.finish(run, end).unwrap();
    }
    sample::builder(FakeVcs::default()).history(history.clone(), history).build().unwrap()
}

/// A report snapshot whose one kept exchange answered `users#get` with 200 and `body`.
fn answering(body: &str) -> Vec<u8> {
    let bytes = sample_snapshot(FAILED_AT + 1).to_json().unwrap();
    let mut snapshot: Value = serde_json::from_slice(&bytes).unwrap();
    let exchange = &mut snapshot["tests"][FAILED_AT]["exchanges"][0];
    for (field, value) in [
        ("api", json!("users#get")),
        ("status", json!(200)),
        ("responseBody", json!(body)),
        ("truncated", json!(false)),
    ] {
        exchange[field] = value;
    }
    serde_json::to_vec(&snapshot).unwrap()
}

#[test]
fn compares_two_stored_runs_with_the_response_shapes_they_changed() {
    let tests = (
        vec![row("users.crud#getsUser", TestStatus::Passed, 5.0)],
        vec![row("users.crud#getsUser", TestStatus::Passed, 5.0)],
    );
    let snapshots = (answering(r#"{"id": 1, "name": "Ada"}"#), answering(r#"{"id": "1", "email": "a@x"}"#));
    let comparison = engine(tests, snapshots).compare_runs(("1", "2"), SLOWER).unwrap();
    assert_eq!((comparison.before, comparison.after, comparison.unchanged()), (1, 2, true));
    let [shape] = comparison.shapes.as_slice() else { panic!("{comparison:?}") };
    assert_eq!((shape.api_id.as_str(), shape.status), ("users#get", 200));
    assert_eq!(
        (shape.added.as_slice(), shape.removed.as_slice()),
        (["/email".to_owned()].as_slice(), ["/name".to_owned()].as_slice())
    );
    assert_eq!(
        (shape.retyped[0].path.as_str(), shape.retyped[0].before.as_str(), shape.retyped[0].after.as_str()),
        ("/id", "number", "string")
    );
}

#[test]
fn refuses_a_load_run_a_missing_run_and_a_threshold_below_one() {
    let engine = engine((Vec::new(), Vec::new()), (Vec::new(), Vec::new()));
    let refused = |runs: (&str, &str), slower: f64| engine.compare_runs(runs, slower).unwrap_err();
    assert!(matches!(refused(("1", "3"), SLOWER), EngineError::History { reason, .. } if reason.contains("load run")));
    assert!(matches!(refused(("1", "9"), SLOWER), EngineError::History { .. }));
    for slower in [0.5, f64::NAN, f64::INFINITY] {
        assert!(matches!(refused(("1", "2"), slower), EngineError::Setting { .. }), "{slower}");
    }
    assert_eq!(
        engine.compare_runs(("1", "2"), 1.0).unwrap(),
        RunComparison { before: 1, after: 2, ..RunComparison::default() }
    );
}
