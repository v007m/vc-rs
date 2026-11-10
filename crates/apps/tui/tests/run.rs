//! The Run tab: `a` asks for a watched run, the feed fills the tests by file, the chosen test
//! shows its checks, and `r`, `R` and `f` rerun and filter what ran.

mod support;

use crossterm::event::KeyCode;
use insta::assert_snapshot;
use support::{carry, press, run_of, screen, tui};
use vc_engine::{RunFeed, SelectRequest};
use vc_tui::{Ask, Update};

const SIZE: (u16, u16) = (100, 24);

#[test]
fn a_runs_every_test_under_the_profiles_and_only_one_run_at_a_time() {
    let mut tui = tui();
    let request = run_of(press(&mut tui, &[KeyCode::Char('a')])).unwrap();
    assert_eq!(request.plan.profiles, ["staging"]);
    assert_eq!((request.plan.select, request.failed, request.watch), (SelectRequest::default(), false, None));
    assert_eq!(press(&mut tui, &[KeyCode::Char('a')]), None, "a run is already starting");
    assert_eq!(press(&mut tui, &[KeyCode::Char('R')]), None);
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), Some(Ask::Stop));
    assert!(screen(&tui, SIZE).lines().nth(1).unwrap_or_default().starts_with("starting…"));
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
