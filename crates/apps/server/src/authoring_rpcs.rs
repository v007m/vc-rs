//! The authoring RPCs: `Call`, `SaveCallAsTest` and `Import`. The web UI's call goes through
//! the profiles' agent policy and the agent log as `web-ui` (D178). An import reads only the
//! text the request carries, never a URL or path, and writes only when asked.

use std::sync::Arc;

use serde_json::Value;
use vc_contracts::engine::v1 as wire;
use vc_engine::{CallAsTest, CallRequest, Changes, FileChange, ImportRequest, Keep};

use crate::engine_api::{EngineApi, Rpcs, failed};
use crate::{Code, ConnectError};

/// The agent name the web UI's calls are checked and logged under.
const WEB_UI: &str = "web-ui";

/// The name a pasted spec goes by: its relative references resolve from the project root.
const PASTED: &str = "pasted";

/// Adds the authoring RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("Call", call).unary("SaveCallAsTest", save_call_as_test).unary("Import", import)
}

async fn call(api: Arc<EngineApi>, request: wire::CallRequest) -> Result<wire::CallResponse, ConnectError> {
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let call = CallRequest {
        api: request.api,
        vars: request.vars,
        keep: request.keep,
        profiles,
        sets,
        agent: Some(WEB_UI.to_owned()),
    };
    let report = api.engine().call(&call).await.map_err(|error| failed(&error))?;
    Ok(wire::CallResponse {
        exit: u32::from(report.exit),
        text: report.text,
        notices: report.notices,
        response_json: report.response.map(|response| response.to_string()),
    })
}

async fn save_call_as_test(
    api: Arc<EngineApi>,
    request: wire::SaveCallAsTestRequest,
) -> Result<wire::SaveCallAsTestResponse, ConnectError> {
    let response = request.response_json.as_deref().map(serde_json::from_str::<Value>).transpose();
    let response = response.map_err(|_| ConnectError::new(Code::InvalidArgument, "the response is not JSON"))?;
    let call = CallAsTest { id: request.id, api: request.api, vars: request.vars, response, fields: request.fields };
    let added = api
        .blocking(move |engine| {
            let added = engine.save_call_as_test(&call)?;
            engine.write(&added.changes)?;
            Ok(added)
        })
        .await?;
    Ok(wire::SaveCallAsTestResponse {
        id: added.id,
        path: added.path,
        line: u32::try_from(added.line).unwrap_or(u32::MAX),
        titles: added.changes.titles,
    })
}

async fn import(api: Arc<EngineApi>, request: wire::ImportRequest) -> Result<wire::ImportResponse, ConnectError> {
    let write = request.write;
    let profiles = api.layers(request.profiles, Vec::new()).0;
    let keep = request.keep.unwrap_or_default();
    let import = ImportRequest {
        kind: request.kind,
        source: PASTED.to_owned(),
        text: Some(request.source),
        file_id: request.file_id,
        tests: request.tests,
        negative: request.negative,
        auth: request.auth,
        include: request.include,
        only: request.only,
        profiles,
        flow: request.flow,
        keep: Keep { hosts: keep.hosts, assets: keep.assets },
    };
    let changes = api.engine().import(&import).await.map_err(|error| failed(&error))?;
    let changes =
        if write { api.blocking(move |engine| engine.write(&changes).map(|()| changes)).await? } else { changes };
    Ok(response(changes))
}

fn response(changes: Changes) -> wire::ImportResponse {
    wire::ImportResponse { files: changes.files.into_iter().map(file_change).collect(), titles: changes.titles }
}

fn file_change(change: FileChange) -> wire::FileChange {
    wire::FileChange { path: change.path, before: change.before, after: change.after, moved_from: change.moved_from }
}

#[cfg(test)]
#[path = "authoring_rpcs_tests.rs"]
mod tests;
