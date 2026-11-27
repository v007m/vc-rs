//! The schedule RPCs (D520, D705–D707): `ListSchedules`, `WriteSchedule` and `ExportSchedule`.
//! A write changes the config only through the engine's guarded preview and digest, as a skip
//! does; an export writes one new workflow at the path the engine fixes, never one a client
//! names. Neither starts a run: only `vc ui`'s scheduler does, from the compiled config.

use std::sync::Arc;

use vc_contracts::engine::v1 as wire;
use vc_engine::{NotifyChannel, ScheduleEdit, ScheduleListing, ScheduleRunner, ScheduleSpec};

use crate::authoring_rpcs::{previewed, response};
use crate::engine_api::{EngineApi, Rpcs};
use crate::{Code, ConnectError, wire_history};

/// Adds the schedule RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("ListSchedules", list_schedules)
        .unary("WriteSchedule", write_schedule)
        .unary("ExportSchedule", export_schedule)
}

/// The config's schedules, each with when it next fires and its newest run.
pub async fn list_schedules(
    api: Arc<EngineApi>,
    _request: wire::ListSchedulesRequest,
) -> Result<wire::ListSchedulesResponse, ConnectError> {
    let listed = api.blocking(vc_engine::Engine::schedules).await?;
    Ok(wire::ListSchedulesResponse { schedules: listed.iter().map(listing).collect() })
}

/// Adds, replaces or removes one schedule: a preview with its digest, or the config written when
/// asked with the digest the preview gave (D706).
pub async fn write_schedule(
    api: Arc<EngineApi>,
    request: wire::WriteScheduleRequest,
) -> Result<wire::WriteScheduleResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let edit = match (request.schedule, request.remove) {
        (Some(schedule), None) => ScheduleEdit::Put(spec(schedule)?),
        (None, Some(id)) => ScheduleEdit::Remove(id),
        _ => return Err(ConnectError::new(Code::InvalidArgument, "set either `schedule` or `remove`")),
    };
    let (changes, digest) =
        api.blocking(move |engine| previewed(engine, engine.schedule_write(&edit)?, expected)).await?;
    let wire::ImportResponse { files, titles } = response(changes);
    Ok(wire::WriteScheduleResponse { files, titles, digest })
}

/// The GitHub Actions workflow that runs one schedule: a preview, or the file written (D707).
pub async fn export_schedule(
    api: Arc<EngineApi>,
    request: wire::ExportScheduleRequest,
) -> Result<wire::ExportScheduleResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let id = request.id;
    let (path, (changes, digest)) = api
        .blocking(move |engine| {
            let export = engine.schedule_export(&id)?;
            Ok((export.path, previewed(engine, export.changes, expected)?))
        })
        .await?;
    let wire::ImportResponse { files, titles } = response(changes);
    Ok(wire::ExportScheduleResponse { files, titles, digest, path })
}

fn listing(listed: &ScheduleListing) -> wire::ScheduleListing {
    wire::ScheduleListing {
        schedule: Some(schedule(&listed.spec)),
        next_ms: listed.next_ms,
        last: listed.last.as_ref().map(wire_history::run),
    }
}

fn schedule(spec: &ScheduleSpec) -> wire::Schedule {
    let notify = spec.notify.iter().map(|channel| wire::NotifyChannel {
        kind: channel.kind.clone(),
        settings: channel.settings.iter().map(|(key, value)| setting(key, value)).collect(),
    });
    wire::Schedule {
        id: spec.id.clone(),
        name: spec.name.clone(),
        cron: spec.cron.clone(),
        selector: spec.selector.clone(),
        profiles: spec.profiles.clone(),
        notify: notify.collect(),
        runner: spec.runner.name().to_owned(),
        enabled: spec.enabled,
    }
}

fn setting(key: &str, value: &str) -> wire::NotifySetting {
    wire::NotifySetting { key: key.to_owned(), value: value.to_owned() }
}

/// The schedule a client sent; an unknown `runner` is refused, an empty one is the default.
fn spec(schedule: wire::Schedule) -> Result<ScheduleSpec, ConnectError> {
    let runner = if schedule.runner.is_empty() {
        ScheduleRunner::default()
    } else {
        ScheduleRunner::named(&schedule.runner)
            .ok_or_else(|| ConnectError::new(Code::InvalidArgument, "`runner` is `vcServer` or `githubActions`"))?
    };
    let notify = schedule.notify.into_iter().map(|channel| NotifyChannel {
        kind: channel.kind,
        settings: channel.settings.into_iter().map(|setting| (setting.key, setting.value)).collect(),
    });
    Ok(ScheduleSpec {
        id: schedule.id,
        name: schedule.name,
        cron: schedule.cron,
        selector: schedule.selector,
        profiles: schedule.profiles,
        notify: notify.collect(),
        runner,
        enabled: schedule.enabled,
    })
}

#[cfg(test)]
#[path = "schedule_rpcs_tests.rs"]
mod tests;
