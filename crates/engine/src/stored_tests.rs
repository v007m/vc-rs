use std::path::PathBuf;
use std::sync::Arc;

use vc_interfaces::{HistoryWriter as _, Row, RunEnd, RunStart, TestRow};
use vc_model::{RunStatus, TestStatus, Totals};
use vc_testkit::{FakeVcs, MemoryHistory};

use super::{ReportRequest, report_choice, started};
use crate::{Engine, ReportFlags, TestTrend, sample};

/// 2026-11-05 10:22 UTC.
const STARTED: u64 = 1_793_874_120_000;

/// The sample engine over a history with a finished run 1 and a run 2 that is still going.
fn engine() -> Engine {
    let history = Arc::new(MemoryHistory::new());
    let start = RunStart {
        started_ms: STARTED,
        profiles: vec!["staging".into()],
        selector: "users.crud".into(),
        ..RunStart::default()
    };
    let first = history.begin(&start).unwrap();
    let failed = TestRow {
        test_id: "users.crud#rejectsBadEmail".into(),
        example_row: Some(1),
        status: Some(TestStatus::Failed),
        message: Some("expected 400, got 201".into()),
        ..TestRow::default()
    };
    let passed =
        TestRow { test_id: "users.crud#listsUsers".into(), status: Some(TestStatus::Passed), ..TestRow::default() };
    history.record(first, vec![Row::Test(failed), Row::Test(passed)]).unwrap();
    let totals = Totals { passed: 1, failed: 1, ..Totals::default() };
    let end = RunEnd { status: RunStatus::Failed, totals, finished_ms: STARTED + 1_000, snapshot: Vec::new() };
    history.finish(first, end).unwrap();
    history.begin(&RunStart { started_ms: STARTED + 60_000, ..RunStart::default() }).unwrap();
    sample::builder(FakeVcs::default()).history(history.clone(), history).build().unwrap()
}

#[test]
fn lists_the_runs_newest_first() {
    let report = engine().history(10).unwrap();
    let text = report.text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].starts_with("    2  2026-11-05 10:23Z  running"), "{text}");
    assert!(
        lines[1].contains("failed     1 passed · 1 failed · 0 blocked · 0 skipped  profile staging  users.crud"),
        "{text}"
    );
    let json = report.json();
    assert_eq!((json[1]["id"].as_u64(), json[1]["status"].as_str()), (Some(1), Some("failed")));
    assert_eq!(json[1]["totals"]["failed"], 1);
}

#[test]
fn hands_screens_the_stored_rows() {
    let engine = engine();
    let runs = engine.history(10).unwrap();
    let ids: Vec<u64> = runs.runs().iter().map(|run| run.id.get()).collect();
    assert_eq!(ids, [2, 1]);
    let shown = engine.stored("1").unwrap();
    assert_eq!((shown.run().id.get(), shown.run().status), (1, Some(RunStatus::Failed)));
    let tests: Vec<&str> = shown.tests().iter().map(|test| test.test_id.as_str()).collect();
    assert_eq!(tests, ["users.crud#rejectsBadEmail", "users.crud#listsUsers"]);
}

#[test]
fn shows_a_run_and_the_tests_that_did_not_pass() {
    let shown = engine().stored("1").unwrap();
    let text = shown.text();
    assert!(text.contains("failed   users.crud#rejectsBadEmail [row 2]\n           expected 400, got 201"), "{text}");
    assert!(!text.contains("listsUsers"), "{text}");
    assert_eq!(shown.json()["tests"][1]["status"], "passed");
    assert_eq!(engine().stored("9").unwrap_err().to_string(), "history: no run 9 is stored");
    assert_eq!(engine().stored("x").unwrap_err().to_string(), "history: `x` is not a run");
}

#[test]
fn reports_need_a_stored_snapshot() {
    let request = ReportRequest { run: "2".into(), ..ReportRequest::default() };
    assert_eq!(engine().report(&request).unwrap_err().to_string(), "history: run 2 has no report snapshot");
}

#[test]
fn report_formats_default_to_html_and_go_in_the_run_folder() {
    let chosen = report_choice(&ReportFlags::default()).unwrap();
    assert_eq!((chosen.files, chosen.single_file), (vec!["html".to_owned()], false));
    let flags = ReportFlags { formats: vec!["ctrf, junit".into()], single_file: true, ..ReportFlags::default() };
    let chosen = report_choice(&flags).unwrap();
    assert_eq!((chosen.files.len(), chosen.single_file), (2, true));
    let flags = ReportFlags { out: Some("/tmp/r".into()), ..ReportFlags::default() };
    assert_eq!(report_choice(&flags).unwrap().flat(true).dir, PathBuf::from("/tmp/r"));
    let bad = ReportFlags { formats: vec!["spec".into()], ..ReportFlags::default() };
    assert_eq!(report_choice(&bad).unwrap_err().to_string(), "--format: `spec` is a console format");
}

#[test]
fn start_times_read_as_utc_minutes() {
    assert_eq!(started(STARTED), "2026-11-05 10:22Z");
    assert_eq!(started(0), "1970-01-01 00:00Z");
}

#[test]
fn trends_sum_up_each_test_over_the_newest_runs() {
    let history = Arc::new(MemoryHistory::new());
    let run = history.begin(&RunStart { kind: "run".into(), ..RunStart::default() }).unwrap();
    let flaky = TestRow {
        test_id: "users.crud#listsUsers".into(),
        status: Some(TestStatus::Flaky),
        duration_ms: 8.0,
        ..TestRow::default()
    };
    history.record(run, vec![Row::Test(flaky)]).unwrap();
    let engine = sample::builder(FakeVcs::default()).history(history.clone(), history).build().unwrap();
    let trend = TestTrend { test_id: "users.crud#listsUsers".into(), results: 1, failed: 0, flaky: 1, mean_ms: 8.0 };
    assert_eq!(engine.trends(10).unwrap(), vec![trend]);
}
