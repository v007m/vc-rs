//! The API client's RPCs (D245, D251): `ReadDefinition`, an api or test as its file writes it
//! for the Client's forms to start from; `ExplainApi`, how an api would be sent; and the draft
//! a `Call` sends in place of the file. They read only a file the editor opens, mask what a
//! run masks, and write nothing; saving a draft is an `EditFile`.

use std::future::{Ready, ready};
use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::CallDraft;

use crate::ConnectError;
use crate::edit_rpcs::pointer_edit;
use crate::engine_api::{EngineApi, Rpcs};

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

/// The layered vars, inherited headers, auth and curl command come with the engine's call
/// explainer; until then they are empty.
fn explain_api(
    _api: Arc<EngineApi>,
    _request: wire::ExplainApiRequest,
) -> Ready<Result<wire::ExplainApiResponse, ConnectError>> {
    ready(Ok(wire::ExplainApiResponse::default()))
}

/// A call's draft as the engine takes it; an edit with no op is refused by its place.
pub(crate) fn draft(draft: &wire::ApiDraft) -> Result<CallDraft, ConnectError> {
    let edits = draft.edits.iter().enumerate().map(pointer_edit).collect::<Result<Vec<_>, _>>()?;
    Ok(CallDraft { path: draft.path.clone(), digest: draft.digest.clone(), edits })
}

#[cfg(test)]
#[path = "client_rpcs_tests.rs"]
mod tests;
