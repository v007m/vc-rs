//! The Run tab: `a` asks for a watched run, the feed fills the tests by file, the chosen test
//! shows its checks, and `r`, `R` and `f` rerun and filter what ran.

mod support;

use crossterm::event::KeyCode;
use insta::assert_snapshot;
use support::{carry, engine, feed_of, press, run_of, screen, tui};
use vc_engine::{Event, OwnerKey, RunFeed, SelectRequest, Sequenced, TestStatus};
use vc_tui::{Ask, Flight, Update};

const SIZE: (u16, u16) = (100, 24);

#[test]
fn a_runs_every_test_under_the_profiles_and_only_one_run_at_a_time() {
    let mut tui = tui();
    let request = run_of(press(&mut tui, &[KeyCode::Char('a')])).unwrap();
    assert_eq!(request.plan.profiles, ["staging"]);
    assert_eq!((request.plan.select, request.failed, request.watch), (SelectRequest::default(), false, None));
    assert_eq!(press(&mut tui, &[KeyCode::Char('a')]), None, "a run is already starting");
    assert_eq!(press(&mut tui, &[KeyCode::Char('R')]), None);
    assert!(screen(&tui, SIZE).lines().nth(1).unwrap_or_default().starts_with("starting…"));
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), Some(Ask::Stop(Flight::Run)));
}

#[test]
fn x_says_the_run_is_stopping_and_the_footer_says_a_second_x_skips_its_cleanups() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('a')]);
    assert!(!screen(&tui, SIZE).contains("again: skip cleanups"), "nothing is stopping yet");
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), Some(Ask::Stop(Flight::Run)));
    let shown = screen(&tui, SIZE);
    assert!(shown.lines().nth(1).unwrap_or_default().starts_with("stopping…"), "{shown}");
    assert!(shown.lines().last().unwrap_or_default().contains("x  again: skip cleanups"), "{shown}");
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), Some(Ask::Stop(Flight::Run)), "as the footer says");
}

#[tokio::test]
async fn a_run_refused_while_another_is_in_progress_leaves_the_last_ones_results_shown() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    carry(&mut tui, ask).await.unwrap();
    press(&mut tui, &[KeyCode::Down, KeyCode::Down]);
    let before = screen(&tui, SIZE);
    assert!(matches!(press(&mut tui, &[KeyCode::Char('a')]), Some(Ask::Run(_))));
    tui.update(&Update::RunFailed("a run or load run is in progress: x on its tab stops it".to_owned()));
    let after = screen(&tui, SIZE);
    assert_eq!(
        after.lines().nth(1).unwrap_or_default(),
        "error a run or load run is in progress: x on its tab stops it"
    );
    assert!(after.lines().skip(2).eq(before.lines().skip(2)), "the results and the chosen test stay:\n{after}");
}

#[tokio::test]
async fn a_watched_run_lists_the_tests_by_file_and_the_chosen_ones_checks() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    carry(&mut tui, ask).await.unwrap();
    assert_snapshot!("run_done", screen(&tui, SIZE));
    press(&mut tui, &[KeyCode::Down, KeyCode::Down]);
    assert_snapshot!("run_failed_test", screen(&tui, SIZE));
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), None, "nothing to stop");
    let rerun = run_of(press(&mut tui, &[KeyCode::Char('r')])).unwrap();
    assert_eq!((rerun.plan.select.ids, rerun.failed), (vec!["users.crud#createsUser".to_owned()], false));
}

#[tokio::test]
async fn f_lists_only_the_failures_and_big_r_reruns_them() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    carry(&mut tui, ask).await.unwrap();
    press(&mut tui, &[KeyCode::Char('f'), KeyCode::Down]);
    let shown = screen(&tui, SIZE);
    assert!(shown.contains("Tests · 2 files · failures only") && shown.contains("✕ createsUser"), "{shown}");
    assert!(!shown.contains("listsUsers") && !shown.contains("orders.flow"), "{shown}");
    let rerun = run_of(press(&mut tui, &[KeyCode::Char('R')])).unwrap();
    assert_eq!((rerun.plan.select, rerun.failed), (SelectRequest::default(), true));
}

#[test]
fn a_run_that_fails_shows_why_and_a_new_one_can_start() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('a')]);
    tui.update(&Update::RunFailed("unknown profile `prod`\n  help: the profiles are staging".to_owned()));
    let status = screen(&tui, SIZE).lines().nth(1).unwrap_or_default().to_owned();
    assert_eq!(status, "error unknown profile `prod`");
    assert!(matches!(press(&mut tui, &[KeyCode::Char('a')]), Some(Ask::Run(_))));
}

#[test]
fn missed_events_are_counted_and_control_characters_never_reach_the_terminal() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('a')]);
    tui.update(&Update::Run(RunFeed::Missed { count: 3 }));
    tui.update(&Update::RunFailed("bad\u{1b}[2Jname".to_owned()));
    let status = screen(&tui, SIZE).lines().nth(1).unwrap_or_default().to_owned();
    assert_eq!(status, "error bad·[2Jname  missed 3 events; `vc history` has them");
}

#[tokio::test]
async fn the_request_count_is_every_request_sent_not_only_those_the_pane_keeps() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    for item in feed_of(&engine().unwrap(), ask).await.unwrap() {
        let times =
            if matches!(&item, RunFeed::Event(Sequenced { event: Event::Exchanged { .. }, .. })) { 100 } else { 1 };
        for _ in 0..times {
            tui.update(&Update::Run(item.clone()));
        }
    }
    let status = screen(&tui, SIZE).lines().nth(1).unwrap_or_default().to_owned();
    assert!(status.contains("  300 requests"), "{status}");
}

#[tokio::test]
async fn a_retried_test_shows_the_checks_of_its_last_attempt_only() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    for item in retried(feed_of(&engine().unwrap(), ask).await.unwrap()) {
        tui.update(&Update::Run(item));
    }
    press(&mut tui, &[KeyCode::Down, KeyCode::Down]);
    let shown = screen(&tui, SIZE);
    assert_eq!(shown.matches("✕ status :8").count(), 1, "{shown}");
    assert!(shown.contains("  6 requests"), "{shown}");
}

/// The feed as it would be had each test instance run twice, as `retry` runs a failed one:
/// its events come again before it finishes.
fn retried(items: Vec<RunFeed>) -> Vec<RunFeed> {
    let (mut attempt, mut retried) = (Vec::new(), Vec::new());
    for item in items {
        match &item {
            RunFeed::Event(Sequenced { event: Event::TestFinished { .. }, .. }) => retried.append(&mut attempt),
            RunFeed::Event(Sequenced { event, .. }) if of_an_instance(event) => attempt.push(item.clone()),
            _ => {}
        }
        retried.push(item);
    }
    retried
}

const fn of_an_instance(event: &Event) -> bool {
    let (Event::ActionStarted { owner, .. }
    | Event::ActionFinished { owner, .. }
    | Event::Exchanged { owner, .. }
    | Event::CheckEvaluated { owner, .. }
    | Event::Warned { owner, .. }) = event
    else {
        return false;
    };
    matches!(owner, OwnerKey::Instance(_))
}

#[tokio::test]
async fn a_finished_runs_notices_are_shown_on_the_status_line() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    for mut item in feed_of(&engine().unwrap(), ask).await.unwrap() {
        if let RunFeed::Done(report) = &mut item {
            report.notices.push("report junit could not be written: read-only".to_owned());
        }
        tui.update(&Update::Run(item));
    }
    let status = screen(&tui, (140, 24)).lines().nth(1).unwrap_or_default().to_owned();
    assert!(status.ends_with("run #1  warning: report junit could not be written: read-only"), "{status}");
}

#[tokio::test]
async fn tests_skipped_at_run_time_count_as_finished() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    for mut item in feed_of(&engine().unwrap(), ask).await.unwrap() {
        if let RunFeed::Event(Sequenced { event: Event::TestFinished { result, .. }, .. }) = &mut item
            && result.instance.is_some()
        {
            result.status = TestStatus::Skipped;
        }
        if !matches!(item, RunFeed::Done(_)) {
            tui.update(&Update::Run(item));
        }
    }
    let status = screen(&tui, SIZE).lines().nth(1).unwrap_or_default().to_owned();
    let counts = status.split_whitespace().nth(1).unwrap_or_default();
    let (finished, planned) = counts.split_once('/').unwrap_or_default();
    assert!(status.starts_with("running ") && planned != "0" && finished == planned, "{status}");
}

#[tokio::test]
async fn allowed_failures_count_as_finished() {
    let mut tui = tui();
    let ask = press(&mut tui, &[KeyCode::Char('a')]);
    for mut item in feed_of(&engine().unwrap(), ask).await.unwrap() {
        if let RunFeed::Event(Sequenced { event: Event::TestFinished { result, .. }, .. }) = &mut item
            && result.instance.is_some()
        {
            result.status = TestStatus::AllowedFailure;
        }
        if !matches!(item, RunFeed::Done(_)) {
            tui.update(&Update::Run(item));
        }
    }
    let status = screen(&tui, SIZE).lines().nth(1).unwrap_or_default().to_owned();
    let counts = status.split_whitespace().nth(1).unwrap_or_default();
    let (finished, planned) = counts.split_once('/').unwrap_or_default();
    assert!(status.starts_with("running ") && planned != "0" && finished == planned, "{status}");
}
