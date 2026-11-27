use std::fs;
use std::time::Duration;

use vc_contracts::engine::v1 as wire;

use super::{Running, fire};
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
    let mut running = Running::new();
    let started = fire(&api, 2 * HOUR, &mut running).await;
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
    let mut running = Running::from([("nightly".to_owned(), held)]);
    assert_eq!(fire(&api, 2 * HOUR, &mut running).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_project_with_errors_starts_nothing() {
    let (dir, api) = scheduled();
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{CONFIG}{SCHEDULES}  - id: broken\n    cron: soon\n"))
        .unwrap();
    assert_eq!(fire(&api, 2 * HOUR, &mut Running::new()).await, Vec::<String>::new());
}
