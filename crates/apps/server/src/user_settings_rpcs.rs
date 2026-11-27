//! The user config RPCs (D711, D712): `ReadUserSettings` and `WriteUserSettings`. The file is the
//! one the engine fixes, never one a client names; a write is a preview with its digest, then the
//! file written with that digest, and every value the server sends back is masked.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::{UserSettingEdit, UserSettings, UserSettingsChange};

use crate::ConnectError;
use crate::engine_api::{EngineApi, Rpcs};

/// Adds the user config RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("ReadUserSettings", read_user_settings).unary("WriteUserSettings", write_user_settings)
}

/// The user config's settings, masked, each saying whether this project overrides it.
pub async fn read_user_settings(
    api: Arc<EngineApi>,
    _request: wire::ReadUserSettingsRequest,
) -> Result<wire::ReadUserSettingsResponse, ConnectError> {
    let read = api.blocking(|engine| Handle::current().block_on(engine.user_settings())).await?;
    Ok(listing(read))
}

/// Sets or removes settings: a preview with the file's digest, or the file written when asked
/// with the digest the preview gave.
pub async fn write_user_settings(
    api: Arc<EngineApi>,
    request: wire::WriteUserSettingsRequest,
) -> Result<wire::WriteUserSettingsResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let edits: Vec<UserSettingEdit> =
        request.edits.into_iter().map(|edit| UserSettingEdit { key: edit.key, value: edit.value }).collect();
    let change = api
        .blocking(move |engine| Handle::current().block_on(engine.user_settings_write(&edits, expected.as_deref())))
        .await?;
    Ok(changed(change))
}

fn listing(read: UserSettings) -> wire::ReadUserSettingsResponse {
    let settings = read.entries.into_iter().map(|entry| wire::UserSetting {
        key: entry.key,
        value: entry.value,
        shadowed: entry.shadowed,
    });
    wire::ReadUserSettingsResponse { path: read.path, digest: read.digest, settings: settings.collect() }
}

fn changed(change: UserSettingsChange) -> wire::WriteUserSettingsResponse {
    wire::WriteUserSettingsResponse {
        path: change.path,
        before: change.before,
        after: change.after,
        digest: change.digest,
    }
}

#[cfg(test)]
#[path = "user_settings_rpcs_tests.rs"]
mod tests;
