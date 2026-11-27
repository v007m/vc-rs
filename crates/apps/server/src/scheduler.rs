//! `vc ui`'s scheduler (D703): at the start of each minute it asks the engine which schedules
//! fire then and runs each one that is not still running from an earlier minute, as a person's
//! `Run` would run it, over the server's own profiles and `--set` values. It takes nothing from a
//! client: what it runs is the compiled config, which a client changes only through a guarded
//! write, and its run passes the same `reports.dir` check as `Run` (D685). Each run is recorded
//! with its schedule and can be stopped with `Cancel` under the handle `schedule-<id>`. The
//! scheduled runs still running are this task's own; it ends when the server's API has gone.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::runtime::Handle;
use vc_engine::{DueSchedule, Interrupter, PlanRequest, RunRequest};

use crate::ConnectError;
use crate::engine_api::EngineApi;

const MINUTE_MS: u64 = 60_000;

/// The scheduled runs still running, by schedule ID.
pub type Running = HashMap<String, Interrupter>;

/// Starts the scheduler on the current Tokio runtime; outside one, nothing is scheduled.
pub fn spawn(api: Weak<EngineApi>) {
    if let Ok(runtime) = Handle::try_current() {
        drop(runtime.spawn(tick(api)));
    }
}

/// Fires each minute until the API has gone.
async fn tick(api: Weak<EngineApi>) {
    let mut running = Running::new();
    loop {
        let now = now_ms();
        let minute = (now / MINUTE_MS + 1) * MINUTE_MS;
        tokio::time::sleep(Duration::from_millis(minute.saturating_sub(now))).await;
        let Some(api) = api.upgrade() else { return };
        drop(fire(&api, minute, &mut running).await);
    }
}

/// Starts each schedule due in the minute that starts at `minute_ms` and not still in
/// `running`; the IDs of those started. A project with errors starts none that minute.
pub async fn fire(api: &Arc<EngineApi>, minute_ms: u64, running: &mut Running) -> Vec<String> {
    running.retain(|_, run| !run.is_finished());
    let Ok(due) = api.blocking(move |engine| engine.due_schedules(minute_ms)).await else { return Vec::new() };
    let mut started = Vec::new();
    for schedule in due {
        if running.contains_key(&schedule.id) {
            continue;
        }
        let id = schedule.id.clone();
        if let Ok(run) = start(api, schedule).await {
            drop(running.insert(id.clone(), run));
            started.push(id);
        }
    }
    started
}

/// Starts `due`'s run on a task of its own; what stops it.
async fn start(api: &Arc<EngineApi>, due: DueSchedule) -> Result<Interrupter, ConnectError> {
    let (profiles, sets) = api.layers(due.profiles, Vec::new())?;
    let plan = PlanRequest { select: due.select, repeat: None, profiles, sets };
    let (interrupter, interrupts) = vc_engine::interrupts();
    // Under a handle that is too long, or that a client's run holds, it stops with the server only.
    let _cancelable = api.runs().open(&format!("schedule-{}", due.id), &interrupter).await;
    let request = RunRequest { plan, schedule: Some(due.id), ..RunRequest::default() };
    let api = Arc::clone(api);
    drop(tokio::spawn(async move {
        api.blocking(move |engine| Handle::current().block_on(engine.run(&request, interrupts))).await
    }));
    Ok(interrupter)
}

/// Now, in milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
