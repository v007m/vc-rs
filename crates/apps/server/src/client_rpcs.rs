//! The API client's RPCs (D245, D251): `ReadDefinition`, an api or test as its file writes it
//! for the Client's forms to start from; `ExplainApi`, how an api would be sent; and the draft
//! a `Call` sends in place of the file. They read only a file the editor opens, mask what a
//! run masks, and write nothing; saving a draft is an `EditFile`.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::{CallDraft, CallRequest};

use crate::ConnectError;
use crate::edit_rpcs::pointer_edit;
use crate::engine_api::{EngineApi, Rpcs};
use crate::project_rpcs::config_value;

/// Adds `ReadDefinition` and `ExplainApi`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("ReadDefinition", read_definition).unary("ExplainApi", explain_api)
}

/// The api or test as written, where it sits and its file's digest.
async fn read_definition(
    api: Arc<EngineApi>,
    request: wire::ReadDefinitionRequest,
) -> Result<wire::ReadDefinitionResponse, ConnectError> {
    let id = request.id;
    let written = api.blocking(move |engine| Handle::current().block_on(engine.written_definition(&id))).await?;
    Ok(wire::ReadDefinitionResponse {
        path: written.path,
        pointer: written.pointer,
        digest: written.digest,
        json: written.json,
    })
}

/// The api as `Call` would send it: the vars it uses with their layers, the headers it inherits,
/// its auth and a curl command, every value masked and the token `$TOKEN`.
async fn explain_api(
    api: Arc<EngineApi>,
    request: wire::ExplainApiRequest,
) -> Result<wire::ExplainApiResponse, ConnectError> {
    let (profiles, sets) = api.layers(request.profiles, request.sets);
    let call = CallRequest { api: request.api, vars: request.vars, profiles, sets, ..CallRequest::default() };
    let explained = api.blocking(move |engine| Handle::current().block_on(engine.explain_api(&call))).await?;
    let headers = explained.headers.into_iter().map(|header| wire::InheritedHeader {
        name: header.name,
        value: header.value,
        from: header.from,
    });
    let auth = explained.auth.map(|auth| wire::ApiAuth {
        id: auth.id,
        r#type: auth.kind,
        token_url: auth.token_url.unwrap_or_default(),
        secrets: auth.secrets,
    });
    Ok(wire::ExplainApiResponse {
        vars: explained.vars.into_iter().map(config_value).collect(),
        headers: headers.collect(),
        auth,
        curl: explained.curl,
        notices: explained.notices,
    })
}

/// A call's draft as the engine takes it; an edit with no op is refused by its place.
pub fn draft(draft: &wire::ApiDraft) -> Result<CallDraft, ConnectError> {
    let edits = draft.edits.iter().enumerate().map(pointer_edit).collect::<Result<Vec<_>, _>>()?;
    Ok(CallDraft { path: draft.path.clone(), digest: draft.digest.clone(), edits })
}

#[cfg(test)]
#[path = "client_rpcs_tests.rs"]
mod tests;
