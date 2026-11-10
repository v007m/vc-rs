//! The History tab: it reads the history when first shown, lists the stored runs with their
//! trends and troubled tests, and compares the chosen run with the one before it.

mod support;

use std::sync::Arc;

use crossterm::event::KeyCode;
use insta::assert_snapshot;
use support::{Outcome, carry_on, engine_over, press, screen, tui};
use vc_engine::{Engine, RunFeed, RunId, RunRow, RunStart, RunStatus, TestRow, TestStatus, TestTrend, Totals};
use vc_testkit::MemoryHistory;
use vc_tui::{Ask, Tui, Update};

const SIZE: (u16, u16) = (100, 30);

/// Run `id` on `main`, started an hour after the run before it, taking `took` milliseconds.
fn run(id: u64, totals: Totals, took: u64) -> RunRow {
    let started_ms = 1_762_335_000_000 - (482 - id) * 3_600_000;
    let start = RunStart {
        kind: "run".to_owned(),
        started_ms,
        git_branch: Some("main".to_owned()),
        git_commit: Some(format!("{id}c9e4f1a")),
        ..RunStart::default()
    };
    let status = if totals.failed + totals.blocked == 0 { RunStatus::Passed } else { RunStatus::Failed };
    RunRow { id: RunId::new(id), start, finished_ms: Some(started_ms + took), status: Some(status), totals }
}

const fn totals(passed: usize, failed: usize, blocked: usize, flaky: usize) -> Totals {
    Totals { passed, failed, blocked, flaky, skipped: 0 }
}

fn test(id: &str, status: TestStatus, duration_ms: f64) -> TestRow {
    TestRow { test_id: id.to_owned(), status: Some(status), duration_ms, attempts: 1, ..TestRow::default() }
}

/// Three runs, the newest failing, and two troubled tests.
fn history() -> Update {
    let runs = vec![
        run(482, totals(396, 6, 4, 3), 48_200),
        run(481, totals(407, 2, 0, 1), 51_300),
        run(480, totals(409, 0, 0, 0), 50_900),
    ];
    let trends = vec![
        TestTrend {
            test_id: "catalog.search#fuzzyQueries".to_owned(),
            results: 3,
            failed: 1,
            flaky: 2,
            mean_ms: 840.0,
        },
        TestTrend {
            test_id: "orders.checkout#placesOrder".to_owned(),
            results: 3,
            failed: 1,
            flaky: 0,
            mean_ms: 1_210.0,
        },
        TestTrend { test_id: "users.crud#listsUsers".to_owned(), results: 3, failed: 0, flaky: 0, mean_ms: 120.0 },
    ];
    Update::History { runs, trends }
}

/// The UI on the History tab, the history read.
fn on_history() -> Tui {
    let mut tui = tui();
    assert_eq!(press(&mut tui, &[KeyCode::Char('3')]), Some(Ask::History));
    tui.update(&history());
    tui
}

#[test]
fn the_history_is_read_when_the_tab_is_first_shown_and_again_after_a_run() {
    let mut tui = tui();
    assert_eq!(press(&mut tui, &[KeyCode::Char('3')]), Some(Ask::History));
    assert!(screen(&tui, SIZE).lines().nth(1).unwrap_or_default().starts_with("reading the history…"));
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), None, "it is being read");
    tui.update(&history());
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('3')]), None, "read already");
    tui.update(&Update::Run(RunFeed::Missed { count: 1 }));
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('3')]), None, "no run has finished");
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), Some(Ask::History));
}

#[test]
fn the_runs_trends_and_troubled_tests_show_newest_first() {
    let tui = on_history();
    assert_snapshot!("history_runs", screen(&tui, SIZE));
}

#[test]
fn enter_compares_the_chosen_run_with_the_one_before() {
    let mut tui = on_history();
    press(&mut tui, &[KeyCode::Down, KeyCode::Down, KeyCode::Down]);
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), None, "the oldest run has none before it");
    let ask = press(&mut tui, &[KeyCode::Up, KeyCode::Up, KeyCode::Enter]);
    assert_eq!(ask, Some(Ask::Compare { before: 481, after: 482 }));
    assert!(screen(&tui, SIZE).contains("reading #481 and #482…"));
    let before = vec![test("orders.checkout#placesOrder", TestStatus::Passed, 1_100.0)];
    let after = vec![
        test("orders.checkout#placesOrder", TestStatus::Failed, 1_800.0),
        test("catalog.admin#publishesProduct", TestStatus::Blocked, 0.0),
    ];
    tui.update(&Update::Compared { before: (480, Vec::new()), after: (481, Vec::new()) });
    assert!(screen(&tui, SIZE).contains("reading #481 and #482…"), "an answer to another ask is ignored");
    tui.update(&Update::Compared { before: (481, before), after: (482, after) });
    assert_snapshot!("history_compared", screen(&tui, SIZE));
}

#[test]
fn a_history_that_cannot_be_read_says_why() {
    let mut tui = on_history();
    tui.update(&Update::HistoryFailed("history.db is locked\n  help: close the other vc".to_owned()));
    let status = screen(&tui, SIZE).lines().nth(1).unwrap_or_default().to_owned();
    assert_eq!(status, "error history.db is locked");
}

/// What the terminal would send back for [`Ask::History`].
fn read(engine: &Engine) -> Outcome<Update> {
    let runs = engine.history(30)?.runs().to_vec();
    Ok(Update::History { runs, trends: engine.trends(30)? })
}

#[tokio::test]
async fn two_stored_runs_compare_as_the_engine_stored_them() {
    let history = Arc::new(MemoryHistory::new());
    let mut tui = tui();
    let engine = engine_over(&history, 201).unwrap();
    assert_eq!(press(&mut tui, &[KeyCode::Char('3')]), Some(Ask::History));
    tui.update(&read(&engine).unwrap());
    assert!(screen(&tui, SIZE).contains("no run is stored yet"));
    let ask = press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('a')]);
    carry_on(&engine, &mut tui, ask).await.unwrap();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    let engine = engine_over(&history, 200).unwrap();
    carry_on(&engine, &mut tui, ask).await.unwrap();
    assert_eq!(press(&mut tui, &[KeyCode::Char('3')]), Some(Ask::History), "runs have finished since");
    tui.update(&read(&engine).unwrap());
    let Some(Ask::Compare { before, after }) = press(&mut tui, &[KeyCode::Enter]) else { panic!("enter compares") };
    let tests = |run: u64| (run, engine.stored(&run.to_string()).unwrap().tests().to_vec());
    tui.update(&Update::Compared { before: tests(before), after: tests(after) });
    let shown = screen(&tui, SIZE);
    assert!(shown.contains("2 stored runs, newest first"), "{shown}");
    assert!(shown.contains("Newly failing · 1") && shown.contains("  users.crud#createsUser"), "{shown}");
    assert!(shown.contains("users.crud#createsUser") && shown.contains("↻ 0  ✕ 1  "), "{shown}");
}
