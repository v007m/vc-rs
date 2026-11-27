//! The Settings RPCs for alerts and plugins (D713–D715): `TestAlert`, `PluginList`, `PluginAdd`
//! and `PluginRemove`. A test alert goes through the same layering checks as `Run`. An add or a
//! removal is a preview with its digest, then the plugin installed or uninstalled and the config
//! and lock written with that digest; a client's source is a registry package or a folder inside
//! the project, and no refusal echoes it.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::{ExplainRequest, PluginChange, PluginListing, PluginManifest, PluginPermissions};

use crate::ConnectError;
use crate::authoring_rpcs::response;
use crate::engine_api::{EngineApi, Rpcs};

/// Adds the alert and plugin RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("TestAlert", test_alert)
        .unary("PluginList", plugin_list)
        .unary("PluginAdd", plugin_add)
        .unary("PluginRemove", plugin_remove)
}

/// A sample notice through each alert channel the profiles reach, under the request's profiles
/// and sets over the server's.
pub async fn test_alert(
    api: Arc<EngineApi>,
    request: wire::TestAlertRequest,
) -> Result<wire::TestAlertResponse, ConnectError> {
    let (profiles, sets) = api.layers(request.profiles, request.sets)?;
    let layers = ExplainRequest { profiles, sets };
    let sent = api.blocking(move |engine| Handle::current().block_on(engine.test_alerts(&layers))).await?;
    let sent = sent.into_iter().map(|sent| wire::AlertSent {
        rule: u32::try_from(sent.rule).unwrap_or(u32::MAX),
        channel: sent.channel,
        error: sent.error,
    });
    Ok(wire::TestAlertResponse { sent: sent.collect() })
}

/// Each plugin of the config or the lock, and whether a run can use it.
pub async fn plugin_list(
    api: Arc<EngineApi>,
    _request: wire::PluginListRequest,
) -> Result<wire::PluginListResponse, ConnectError> {
    let listed = api.blocking(vc_engine::Engine::plugin_list).await?;
    Ok(wire::PluginListResponse { plugins: listed.iter().map(listing).collect() })
}

/// A plugin added: a preview, or it installed and written with the preview's digest.
pub async fn plugin_add(
    api: Arc<EngineApi>,
    request: wire::PluginAddRequest,
) -> Result<wire::PluginAddResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let source = request.source;
    let change = api.blocking(move |engine| engine.client_plugin_add(&source, expected.as_deref())).await?;
    let PluginChange { manifest: fetched, resolved, integrity, warnings, changes, digest, .. } = change;
    let wire::ImportResponse { files, titles } = response(changes);
    let manifest = fetched.as_ref().map(manifest);
    Ok(wire::PluginAddResponse { manifest, resolved, integrity, warnings, files, titles, digest })
}

/// A plugin removed: a preview, or it written and uninstalled with the preview's digest.
pub async fn plugin_remove(
    api: Arc<EngineApi>,
    request: wire::PluginRemoveRequest,
) -> Result<wire::PluginRemoveResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let id = request.id;
    let change = api.blocking(move |engine| engine.client_plugin_remove(&id, expected.as_deref())).await?;
    let digest = change.digest;
    let wire::ImportResponse { files, titles } = response(change.changes);
    Ok(wire::PluginRemoveResponse { files, titles, digest })
}

fn listing(listed: &PluginListing) -> wire::PluginListing {
    wire::PluginListing {
        id: listed.id.clone(),
        source: listed.source.clone(),
        manifest: listed.manifest.as_ref().map(manifest),
        granted: Some(permissions(&listed.granted)),
        state: listed.state.word().to_owned(),
        detail: listed.state.to_string(),
    }
}

fn manifest(manifest: &PluginManifest) -> wire::PluginManifest {
    wire::PluginManifest {
        id: manifest.id.clone(),
        version: manifest.version.clone(),
        kind: manifest.kind.name().to_owned(),
        command: manifest.command.clone(),
        args: manifest.args.clone(),
        permissions: Some(permissions(&manifest.permissions)),
    }
}

fn permissions(permissions: &PluginPermissions) -> wire::PluginPermissions {
    wire::PluginPermissions {
        network: permissions.network.clone(),
        secrets: permissions.secrets.clone(),
        files: permissions.files.clone(),
    }
}

#[cfg(test)]
#[path = "plugin_rpcs_tests.rs"]
mod tests;
