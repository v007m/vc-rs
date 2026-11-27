use std::fs;
use std::time::Duration;

use vc_contracts::engine::v1 as wire;

use super::{Fired, fire};
use crate::engine_fixture::{CONFIG, project};
use crate::history_rpcs::list_runs;

const HOUR: u64 = 3_600_000;

const SCHEDULES: &str = "schedules:\n  - id: nightly\n    cron: 0 2 * * *\n    selector: users.crud\n  - id: onGithub\n    cron: 0 2 * * *\n    runner: githubActions\n  - id: paused\n    cron: 0 2 * * *\n    enabled: false\n  - id: hourly\n    cron: 30 * * * *\n";

fn scheduled() -> (tempfile::TempDir, std::sync::Arc<crate::EngineApi>) {
    let (dir, api) = project();
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{CONFIG}{SCHEDULES}")).unwrap();
    (dir, api)
}

fn of(schedule: &str) -> wire::ListRunsRequest {
    wire::ListRunsRequest { schedule: Some(schedule.to_owned()), ..wire::ListRunsRequest::default() }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_due_schedule_runs_once_and_is_recorded_with_its_schedule() {
    let (_dir, api) = scheduled();
    let started = fire(&api, 2 * HOUR, &mut Fired::default()).await;
    assert_eq!(started, ["nightly"], "neither one left to GitHub Actions, a disabled one nor one due at :30");
    let mut runs = Vec::new();
    for _ in 0..200 {
        runs = list_runs(api.clone(), of("nightly")).await.unwrap().runs;
        if !runs.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let [run] = runs.as_slice() else { panic!("{runs:?}") };
    assert_eq!((run.kind.as_str(), run.schedule.as_deref()), ("run", Some("nightly")));
    assert_eq!(list_runs(api, of("hourly")).await.unwrap().runs, []);
}

#[tokio::test]
async fn a_schedule_still_running_is_not_started_again() {
    let (_dir, api) = scheduled();
    let (held, _run) = vc_engine::interrupts();
    let mut fired = Fired { running: [("nightly".to_owned(), held)].into(), ..Fired::default() };
    assert_eq!(fire(&api, 2 * HOUR, &mut fired).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_project_with_errors_starts_nothing() {
    let (dir, api) = scheduled();
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{CONFIG}{SCHEDULES}  - id: broken\n    cron: soon\n"))
        .unwrap();
    assert_eq!(fire(&api, 2 * HOUR, &mut Fired::default()).await, Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_minute_is_never_fired_twice_on_an_early_wake_or_a_clock_stepped_back() {
    let (_dir, api) = scheduled();
    let mut fired = Fired::default();
    let day = 24 * HOUR;
    let nightly = 30 * day + 2 * HOUR;
    let mut clock = [nightly, nightly, nightly - day, nightly + day].into_iter();
    let mut tick = async |fired: &mut Fired| {
        fired.running.clear();
        fire(&api, clock.next().unwrap(), fired).await
    };
    assert_eq!(tick(&mut fired).await, ["nightly"]);
    assert_eq!(tick(&mut fired).await, Vec::<String>::new(), "woken early, the same minute again");
    assert_eq!(tick(&mut fired).await, Vec::<String>::new(), "the wall clock stepped back a day");
    assert_eq!(tick(&mut fired).await, ["nightly"], "the next day's minute");
}
