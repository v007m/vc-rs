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
    Totals { passed, failed, blocked, flaky, skipped: 0, allowed_failures: 0 }
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
fn the_chosen_run_stays_chosen_when_a_reread_puts_a_newer_run_above_it() {
    let mut tui = on_history();
    assert_eq!(press(&mut tui, &[KeyCode::Down, KeyCode::Char('r')]), Some(Ask::History));
    let Update::History { mut runs, trends } = history() else { panic!("the history") };
    runs.insert(0, RunRow { id: RunId::new(483), ..run(482, totals(409, 0, 0, 0), 48_000) });
    tui.update(&Update::History { runs, trends });
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), Some(Ask::Compare { before: 480, after: 481 }));
}

/// Run `id` of `kind`, otherwise as [`run`] makes it.
fn run_of(kind: &str, id: u64, totals: Totals) -> RunRow {
    let mut row = run(id, totals, 30_000);
    kind.clone_into(&mut row.start.kind);
    row
}

#[test]
fn a_run_compares_with_the_run_of_tests_before_it_and_load_runs_stay_out_of_the_trends_and_compares() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('3')]);
    let runs = vec![
        run_of("load", 482, totals(0, 0, 0, 0)),
        run_of("run", 481, totals(396, 6, 4, 3)),
        run_of("load", 480, totals(0, 0, 0, 0)),
        run_of("run", 479, totals(409, 0, 0, 0)),
    ];
    tui.update(&Update::History { runs, trends: Vec::new() });
    assert!(screen(&tui, SIZE).contains("Trends · last 2 runs"), "the load runs ran no tests");
    assert!(screen(&tui, SIZE).contains("#482 is a load run"), "{}", screen(&tui, SIZE));
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), None, "a load run stored no tests to compare");
    assert!(!screen(&tui, SIZE).contains("nothing failing"));
    assert_eq!(press(&mut tui, &[KeyCode::Down, KeyCode::Enter]), Some(Ask::Compare { before: 479, after: 481 }));
    assert_eq!(press(&mut tui, &[KeyCode::Down, KeyCode::Enter]), None, "#480 is a load run");
}

#[test]
fn a_load_run_lists_its_iterations_and_errors_as_vc_history_does_not_a_pass_rate() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('3')]);
    let runs = vec![run_of("load", 482, totals(37, 3, 0, 0)), run_of("run", 481, totals(37, 3, 0, 0))];
    tui.update(&Update::History { runs, trends: Vec::new() });
    let shown = screen(&tui, SIZE);
    let rows: Vec<&str> = shown.lines().filter(|line| line.starts_with("│▸ #") || line.starts_with("│  #")).collect();
    let [load, tests] = rows.as_slice() else { panic!("{shown}") };
    assert!(load.contains("09:30Z load · 40 iterations · 3 errors "), "{shown}");
    assert!(!load.contains("92.5%") && !load.contains(" 37 "), "a load run passed no tests: {shown}");
    assert!(load.trim_end_matches([' ', '│']).ends_with("30.0s"), "{shown}");
    assert!(tests.contains("main@481c9e4") && tests.contains("92.5%") && tests.contains(" 37 "), "{shown}");
}

#[test]
fn a_late_failure_of_an_earlier_compare_leaves_the_newer_one_pending() {
    let mut tui = on_history();
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), Some(Ask::Compare { before: 481, after: 482 }));
    assert_eq!(press(&mut tui, &[KeyCode::Down, KeyCode::Enter]), Some(Ask::Compare { before: 480, after: 481 }));
    tui.update(&Update::CompareFailed { before: 481, after: 482, reason: "run 481 is gone".to_owned() });
    let shown = screen(&tui, SIZE);
    assert!(shown.contains("reading #480 and #481…"), "{shown}");
    assert!(!shown.contains("run 481 is gone"), "{shown}");
    tui.update(&Update::CompareFailed { before: 480, after: 481, reason: "run 480 is gone".to_owned() });
    assert!(screen(&tui, SIZE).contains("error run 480 is gone"));
}

#[test]
fn a_comparison_that_cannot_be_read_says_why_on_its_pane_and_leaves_the_runs_listed() {
    let mut tui = on_history();
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), Some(Ask::Compare { before: 481, after: 482 }));
    let reason = "run 481 is gone\n  help: vc history lists the runs".to_owned();
    tui.update(&Update::CompareFailed { before: 481, after: 482, reason });
    let shown = screen(&tui, SIZE);
    assert!(shown.contains("error run 481 is gone"), "{shown}");
    assert!(shown.lines().nth(1).unwrap_or_default().starts_with("3 stored runs"), "{shown}");
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), Some(Ask::Compare { before: 481, after: 482 }), "it may retry");
    assert!(!screen(&tui, SIZE).contains("error run 481 is gone"));
}

#[test]
fn a_run_that_ends_in_an_error_is_reread_as_it_may_be_stored() {
    let mut tui = on_history();
    tui.update(&Update::RunFailed("the run stopped".to_owned()));
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('3')]), Some(Ask::History));
}

#[test]
fn a_run_that_finishes_while_the_history_is_read_leaves_it_stale() {
    let mut tui = tui();
    assert_eq!(press(&mut tui, &[KeyCode::Char('3')]), Some(Ask::History));
    tui.update(&Update::Loaded(Box::default()));
    tui.update(&history());
    let ask = press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('3')]);
    assert_eq!(ask, Some(Ask::History), "the read was asked for before the run finished");
    tui.update(&history());
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('3')]), None, "read after it finished");
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
