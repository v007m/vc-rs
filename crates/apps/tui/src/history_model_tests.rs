use vc_engine::{RunId, RunRow, RunStart, TestRow, TestStatus, TestTrend, Totals};

use super::{Comparison, Slower, checkout, label, mean, percent, seconds, took, troubled};

fn test(id: &str, status: TestStatus, duration_ms: f64) -> TestRow {
    TestRow { test_id: id.to_owned(), status: Some(status), duration_ms, attempts: 1, ..TestRow::default() }
}

fn trend(id: &str, failed: u32, flaky: u32) -> TestTrend {
    TestTrend { test_id: id.to_owned(), results: 10, failed, flaky, mean_ms: 0.0 }
}

#[test]
fn a_comparison_sorts_each_test_by_how_it_changed() {
    let before = [
        test("a#keepsPassing", TestStatus::Passed, 100.0),
        test("a#breaks", TestStatus::Passed, 100.0),
        test("a#getsBlocked", TestStatus::Flaky, 100.0),
        test("a#getsFixed", TestStatus::Failed, 100.0),
        test("a#staysBroken", TestStatus::Blocked, 100.0),
        test("a#slowsDown", TestStatus::Passed, 100.0),
    ];
    let after = [
        test("a#keepsPassing", TestStatus::Passed, 120.0),
        test("a#breaks", TestStatus::Failed, 100.0),
        test("a#getsBlocked", TestStatus::Blocked, 0.0),
        test("a#getsFixed", TestStatus::Flaky, 100.0),
        test("a#staysBroken", TestStatus::Failed, 100.0),
        test("a#slowsDown", TestStatus::Passed, 121.0),
        test("a#isNew", TestStatus::Failed, 5.0),
    ];
    let comparison = Comparison::new((1, 2), &before, &after);
    assert_eq!((comparison.before, comparison.after), (1, 2));
    assert_eq!(comparison.newly_failing, ["a#breaks", "a#isNew"]);
    assert_eq!(comparison.newly_blocked, ["a#getsBlocked"]);
    assert_eq!(comparison.fixed, ["a#getsFixed"]);
    assert_eq!(comparison.still_failing, ["a#staysBroken"]);
    let slower = Slower { test: "a#slowsDown".to_owned(), before_ms: 100.0, after_ms: 121.0 };
    assert_eq!(comparison.slower, [slower]);
    assert!(!comparison.unchanged());
}

#[test]
fn instances_are_matched_by_row_and_repeat_and_skipped_ones_are_never_slower() {
    let row = |status, index, duration| TestRow { example_row: Some(index), ..test("a#rows", status, duration) };
    let before = [row(TestStatus::Failed, 0, 10.0), row(TestStatus::Skipped, 1, 10.0)];
    let after = [row(TestStatus::Passed, 0, 10.0), row(TestStatus::Passed, 1, 50.0)];
    let comparison = Comparison::new((7, 8), &before, &after);
    assert_eq!(comparison.fixed, ["a#rows [row 1]"]);
    assert_eq!(comparison.slower, Vec::<Slower>::new());
    assert!(Comparison::new((1, 2), &after, &after).unchanged());
}

#[test]
fn a_label_numbers_rows_and_repeats_from_one_and_drops_control_characters() {
    let repeated = TestRow { example_row: Some(2), repeat_index: 1, ..test("a#b\u{1b}c", TestStatus::Passed, 0.0) };
    assert_eq!(label(&repeated), "a#b·c [row 3] (repeat 2)");
}

#[test]
fn the_troubled_tests_come_most_troubled_first_and_untroubled_ones_are_left_out() {
    let trends = [trend("a#calm", 0, 0), trend("a#once", 1, 0), trend("a#flaky", 0, 3), trend("a#also", 1, 0)];
    let ids: Vec<&str> = troubled(&trends).iter().map(|trend| trend.test_id.as_str()).collect();
    assert_eq!(ids, ["a#flaky", "a#once", "a#also"]);
}

#[test]
fn a_runs_figures_read_as_its_row_shows_them() {
    let mut run = RunRow {
        id: RunId::new(3),
        start: RunStart::default(),
        finished_ms: None,
        status: None,
        totals: Totals::default(),
    };
    assert_eq!((percent(&run.totals), took(&run), checkout(&run)), ("–".to_owned(), None, "–".to_owned()));
    run.totals = Totals { passed: 960, flaky: 1, failed: 30, blocked: 9, skipped: 5, allowed_failures: 0 };
    run.start.started_ms = 1_000;
    run.finished_ms = Some(49_250);
    run.start.git_branch = Some("main".to_owned());
    run.start.git_commit = Some("a1c9e4f0b2".to_owned());
    assert_eq!(percent(&run.totals), "96.1%");
    assert_eq!(took(&run).map(seconds), Some("48.2s".to_owned()));
    assert_eq!(checkout(&run), "main@a1c9e4f");
    run.start.git_branch = None;
    assert_eq!(checkout(&run), "@a1c9e4f");
    assert_eq!((seconds(350), mean(349.6), mean(1_840.0)), ("350ms".to_owned(), "350ms".to_owned(), "1.8s".to_owned()));
}
