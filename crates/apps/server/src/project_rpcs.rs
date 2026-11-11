//! The project's RPCs: `GetProject`, `ListDefinitions`, `Check`, the editor's `ReadFile` and
//! `WriteFile`, and `ExplainConfig`, whose values the engine masks (D229). Each reads and
//! compiles the project on the blocking pool, never on an async worker.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1::Span;
use vc_engine::{
    ApiCall, CheckRequest, ConfigValue, ExplainRequest, ListKind, ListRequest, Listed, SaveFile, SelectRequest,
};

use crate::ConnectError;
use crate::engine_api::{EngineApi, Rpcs, message};
use crate::wire_diagnostics::diagnostics;

/// Adds the project's RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("GetProject", get_project)
        .unary("ListDefinitions", list_definitions)
        .unary("Check", check)
        .unary("ReadFile", read_file)
        .unary("WriteFile", write_file)
        .unary("ExplainConfig", explain_config)
}

async fn get_project(
    api: Arc<EngineApi>,
    _: wire::GetProjectRequest,
) -> Result<wire::GetProjectResponse, ConnectError> {
    let info = api.blocking(vc_engine::Engine::project_info).await?;
    Ok(wire::GetProjectResponse {
        name: info.name,
        default_profile: info.defaults.into_iter().next(),
        profiles: info.profiles,
        vc_version: info.vc_version,
    })
}

async fn list_definitions(
    api: Arc<EngineApi>,
    request: wire::ListDefinitionsRequest,
) -> Result<wire::ListDefinitionsResponse, ConnectError> {
    let kind = match request.kind() {
        wire::DefinitionKind::Apis => ListKind::Apis,
        wire::DefinitionKind::Files => ListKind::Files,
        wire::DefinitionKind::Tests | wire::DefinitionKind::Unspecified => ListKind::Tests,
    };
    let list = ListRequest { kind, select: select(request.selection), skipped: request.skipped };
    let report = api.blocking(move |engine| engine.list(&list)).await?;
    Ok(wire::ListDefinitionsResponse {
        definitions: report.items.into_iter().map(definition).collect(),
        notices: report.notices,
    })
}

/// A selection as `vc run` takes it; everything when absent.
pub fn select(selection: Option<wire::Selection>) -> SelectRequest {
    let Some(selection) = selection else { return SelectRequest::default() };
    SelectRequest {
        ids: selection.ids,
        paths: selection.paths,
        tags: selection.tags,
        grep: selection.grep,
        changed: selection.changed,
        include_skipped: selection.include_skipped,
        ..SelectRequest::default()
    }
}

fn definition(item: Listed) -> wire::Definition {
    let span = item.span.map(|span| Span {
        file: item.path.clone(),
        start_line: span.start.line,
        start_column: span.start.column,
        end_line: span.end.line,
        end_column: span.end.column,
    });
    wire::Definition {
        id: item.id,
        name: item.name,
        path: item.path,
        tags: item.tags,
        skipped: item.skipped,
        skip_reason: None,
        skip_until: None,
        skip_expired: false,
        span,
        calls: item.calls.into_iter().map(api_call).collect(),
        steps: item.steps,
        vars: item.vars,
    }
}

/// An api a test calls, in its wire form.
pub fn api_call(call: ApiCall) -> wire::ApiCall {
    wire::ApiCall { api: call.api, method: call.method, path: call.path }
}

async fn check(api: Arc<EngineApi>, request: wire::CheckRequest) -> Result<wire::CheckResponse, ConnectError> {
    let check = CheckRequest { paths: request.paths, overlay: request.overlay.into_iter().collect() };
    let report = api.blocking(move |engine| engine.check(&check)).await?;
    Ok(wire::CheckResponse {
        diagnostics: diagnostics(&report),
        checked: u32::try_from(report.checked).unwrap_or(u32::MAX),
        notices: report.notices,
    })
}

async fn read_file(
    api: Arc<EngineApi>,
    request: wire::ReadFileRequest,
) -> Result<wire::ReadFileResponse, ConnectError> {
    let opened = api.blocking(move |engine| engine.read_file(&request.path)).await?;
    Ok(wire::ReadFileResponse { text: opened.text, digest: opened.digest })
}

async fn write_file(
    api: Arc<EngineApi>,
    request: wire::WriteFileRequest,
) -> Result<wire::WriteFileResponse, ConnectError> {
    let save = SaveFile { path: request.path, text: request.text, expected_digest: request.expected_digest };
    let saved = api.blocking(move |engine| engine.save_file(&save)).await?;
    let (diagnostics, check_error) = match &saved.check {
        Ok(report) => (diagnostics(report), String::new()),
        Err(error) => (Vec::new(), message(error)),
    };
    Ok(wire::WriteFileResponse { digest: saved.digest, diagnostics, check_error })
}

async fn explain_config(
    api: Arc<EngineApi>,
    request: wire::ExplainConfigRequest,
) -> Result<wire::ExplainConfigResponse, ConnectError> {
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let explain = ExplainRequest { profiles, sets };
    let keys = request.keys;
    let found = api.blocking(move |engine| Handle::current().block_on(engine.config_values(&keys, &explain))).await?;
    Ok(wire::ExplainConfigResponse {
        values: found.values.into_iter().map(config_value).collect(),
        notices: found.notices,
    })
}

fn config_value(value: ConfigValue) -> wire::ConfigValue {
    let layers = value.layers.into_iter().map(|layer| wire::ConfigLayer { value: layer.value, source: layer.source });
    wire::ConfigValue { key: value.key, layers: layers.collect() }
}

#[cfg(test)]
#[path = "project_rpcs_tests.rs"]
mod tests;
