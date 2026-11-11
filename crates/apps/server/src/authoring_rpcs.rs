//! The authoring RPCs: `Call`, `SaveCallAsTest` and `Import`. The web UI's call is the person at
//! the browser's, as `vc call`'s is: the profiles' agent policy and the agent log, which are for
//! agents on `/mcp`, do not apply (D241). An import reads only the
//! text the request carries, never a URL or path, and fetches none of its `$ref`s from the
//! network; it writes only when asked. A call and an import read and compile the project on the
//! blocking pool, never on an async worker.

use std::sync::Arc;

use serde_json::Value;
use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1 as results;
use vc_engine::{CallAsTest, CallReport, CallRequest, Changes, Exchange, FileChange, ImportRequest, Keep, cut_body};

use crate::engine_api::{EngineApi, Rpcs};
use crate::{Code, ConnectError, wire_results};

/// The name a pasted spec goes by: its relative references resolve from the project root.
const PASTED: &str = "pasted";

/// Adds the authoring RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("Call", call).unary("SaveCallAsTest", save_call_as_test).unary("Import", import)
}

async fn call(api: Arc<EngineApi>, request: wire::CallRequest) -> Result<wire::CallResponse, ConnectError> {
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let call = CallRequest { api: request.api, vars: request.vars, keep: request.keep, profiles, sets, agent: None };
    let report = api.blocking(move |engine| Handle::current().block_on(engine.call(&call))).await?;
    let exchange = report.exchange.as_ref().map(|exchange| bodied(&report, exchange));
    let checks = report.checks.iter().map(|check| wire_results::check(&report.names, check)).collect();
    let saved = report.saved.iter().map(|saved| wire::SavedValue {
        name: saved.name.clone(),
        from: saved.from.clone(),
        value: saved.value.to_string(),
    });
    Ok(wire::CallResponse {
        exit: u32::from(report.exit),
        text: report.text,
        notices: report.notices,
        response_json: report.response.map(|response| response.to_string()),
        exchange,
        checks,
        saved: saved.collect(),
    })
}

/// The call's exchange with both bodies, whatever `history.bodies` says: the Client shows the
/// call the person just made, and the engine masked the bodies already. Each is cut to
/// `history.maxBodySize`, as the history cuts it, so one large body cannot outgrow the
/// envelope; `body_truncated` says if either was.
fn bodied(report: &CallReport, exchange: &Exchange) -> results::Exchange {
    let max = report.names.program().config.history.max_body();
    let (request_body, request_cut) = cut_body(&exchange.request.body, max);
    let (response_body, response_cut) = cut_body(&exchange.response.body, max);
    results::Exchange {
        request_body: request_body.map(String::into_bytes).unwrap_or_default(),
        response_body: response_body.map(String::into_bytes).unwrap_or_default(),
        body_truncated: request_cut || response_cut,
        ..wire_results::exchange(&report.names, report.action, exchange)
    }
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
    let changes = api
        .blocking(move |engine| {
            let changes = Handle::current().block_on(engine.import(&import))?;
            if write {
                engine.write(&changes)?;
            }
            Ok(changes)
        })
        .await?;
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
