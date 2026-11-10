//! The Load tab: it lists the load scenarios when first shown, runs the chosen one with
//! `enter`, shows each second as `vc load` prints it, stops it with `x`, and gives the summary.

mod support;

use std::time::Duration;

use crossterm::event::KeyCode;
use insta::assert_snapshot;
use support::{Outcome, engine, press, screen, tui};
use tokio::sync::mpsc;
use vc_engine::{Engine, Event, ListKind, ListRequest, LoadEnding, LoadReport, LoadRequest, LoadWatch, interrupts};
use vc_tui::{Ask, Tui, Update};

const SIZE: (u16, u16) = (120, 24);

/// What the terminal would send back for [`Ask::Scenarios`]: the project's load files.
fn scenarios(engine: &Engine) -> Outcome<Update> {
    let files = engine.list(&ListRequest { kind: ListKind::Files, ..ListRequest::default() })?.items;
    Ok(Update::Scenarios(files.into_iter().filter(|file| file.path.ends_with(".load.yaml")).collect()))
}

/// The UI on the Load tab, the scenario `users.peak` listed.
fn on_load(engine: &Engine) -> Outcome<Tui> {
    let mut tui = tui();
    if press(&mut tui, &[KeyCode::Char('4')]) != Some(Ask::Scenarios) {
        return Err("the tab lists the scenarios when first shown".into());
    }
    tui.update(&scenarios(engine)?);
    Ok(tui)
}

/// The status line.
fn status(tui: &Tui) -> String {
    screen(tui, SIZE).lines().nth(1).unwrap_or_default().to_owned()
}

/// The closed second `second`, `breaching` the named thresholds.
fn tick(second: u64, errors: u64, breaching: &[&str]) -> Update {
    let ms = |ms| Duration::from_millis(ms);
    Update::Load(Event::LoadTick {
        second,
        users: 10 * u32::try_from(second).unwrap_or(u32::MAX),
        rps: 40 * second,
        p50: ms(30 + second),
        p95: ms(80 + 4 * second),
        p99: ms(140 + 9 * second),
        errors,
        breaching: breaching.iter().map(|name| (*name).to_owned()).collect(),
    })
}

#[test]
fn the_scenarios_are_listed_when_the_tab_is_first_shown() {
    let mut tui = tui();
    assert_eq!(press(&mut tui, &[KeyCode::Char('4')]), Some(Ask::Scenarios));
    assert_eq!(status(&tui), "reading the scenarios…");
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), None, "they are being read");
    tui.update(&scenarios(&engine().unwrap()).unwrap());
    assert_eq!(status(&tui), "1 scenario · enter runs it, x stops it");
    assert!(screen(&tui, SIZE).contains("▸ users.peak"));
    assert_eq!(press(&mut tui, &[KeyCode::Char('1'), KeyCode::Char('4')]), None, "listed already");
    assert_eq!(press(&mut tui, &[KeyCode::Char('r')]), Some(Ask::Scenarios));
    tui.update(&Update::LoadFailed("peak.load.yaml:2: bad indent\n  help: indent by two".to_owned()));
    assert_eq!(status(&tui), "error peak.load.yaml:2: bad indent");
}

#[test]
fn a_project_without_load_files_says_how_to_add_one() {
    let mut tui = tui();
    press(&mut tui, &[KeyCode::Char('4')]);
    tui.update(&Update::Scenarios(Vec::new()));
    assert_eq!(status(&tui), "no load scenario in the project: a *.load.yaml file adds one");
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), None, "there is nothing to run");
}

#[test]
fn enter_runs_the_chosen_scenario_once_and_x_stops_it() {
    let mut tui = on_load(&engine().unwrap()).unwrap();
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), None, "nothing is running");
    let wanted = LoadRequest { scenario: "users.peak".to_owned(), profiles: vec!["staging".to_owned()], sets: vec![] };
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), Some(Ask::Load(wanted)));
    assert_eq!(status(&tui), "starting users.peak…");
    assert_eq!(press(&mut tui, &[KeyCode::Enter]), None, "one load run at a time");
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), Some(Ask::Stop));
}

#[test]
fn each_second_shows_as_vc_load_prints_it_over_bars_of_the_seconds_kept() {
    let mut tui = on_load(&engine().unwrap()).unwrap();
    press(&mut tui, &[KeyCode::Enter]);
    for second in 1..=11 {
        let breaching: &[&str] = if second > 9 { &["p95"] } else { &[] };
        tui.update(&tick(second, second / 10, breaching));
    }
    assert_eq!(status(&tui), "running users.peak · 11s · x stops it");
    assert_snapshot!("load_live", screen(&tui, SIZE));
}

#[test]
fn the_summary_shows_once_the_run_is_over() {
    let mut tui = on_load(&engine().unwrap()).unwrap();
    press(&mut tui, &[KeyCode::Enter]);
    tui.update(&tick(1, 0, &[]));
    let mut report = LoadReport::default();
    (report.scenario, report.ending, report.exit, report.run_id) =
        ("users.peak".to_owned(), LoadEnding::Finished, 0, Some(7));
    (report.seconds, report.iterations, report.rps) = (30, 1_200, 40.0);
    (report.p50_ms, report.p95_ms, report.p99_ms, report.max_ms) = (31.0, 88.0, 140.0, 212.0);
    tui.update(&Update::Loaded(Box::new(report)));
    assert_eq!(status(&tui), "users.peak finished after 30 s · exit 0 · run #7");
    assert_snapshot!("load_summary", screen(&tui, SIZE));
    assert_eq!(press(&mut tui, &[KeyCode::Char('x')]), None, "the run is over");
}

#[test]
fn a_load_run_that_fails_says_why_and_may_run_again() {
    let mut tui = on_load(&engine().unwrap()).unwrap();
    press(&mut tui, &[KeyCode::Enter]);
    tui.update(&Update::LoadFailed("a run or load run is in progress: x on its tab stops it".to_owned()));
    assert_eq!(status(&tui), "error a run or load run is in progress: x on its tab stops it");
    assert!(matches!(press(&mut tui, &[KeyCode::Enter]), Some(Ask::Load(_))));
}

#[tokio::test]
async fn a_real_load_run_ticks_into_the_tab_and_ends_within_its_thresholds() {
    let engine = engine().unwrap();
    let mut tui = on_load(&engine).unwrap();
    let Some(Ask::Load(request)) = press(&mut tui, &[KeyCode::Enter]) else { panic!("enter runs it") };
    let ((ticks, mut heard), (_interrupter, interrupts)) = (mpsc::channel(64), interrupts());
    let report = engine.load(&request, LoadWatch { interrupts, ticks: Some(ticks) }).await.unwrap();
    while let Some(event) = heard.recv().await {
        tui.update(&Update::Load(event));
    }
    tui.update(&Update::Loaded(Box::new(report)));
    assert!(status(&tui).starts_with("users.peak finished after "), "{}", status(&tui));
    assert!(status(&tui).contains(" · exit 0 · run #"), "{}", status(&tui));
    assert!(screen(&tui, SIZE).contains("latency p50"), "the summary shows");
}
