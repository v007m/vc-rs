//! The API client's RPCs (D245): `ExplainApi`, the api as its file writes it for the Client's
//! form to start from, and the draft a `Call` sends in place of the file. Both read only a file
//! the editor opens, mask what a run masks, and write nothing; saving a draft is an `EditFile`.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::CallDraft;

use crate::ConnectError;
use crate::edit_rpcs::pointer_edit;
use crate::engine_api::{EngineApi, Rpcs};

/// Adds `ExplainApi`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("ExplainApi", explain_api)
}

/// The api as written, where it sits and its file's digest. The layered vars, inherited
/// headers, auth and curl command come with the engine's call explainer; until then they are
/// empty.
async fn explain_api(
    api: Arc<EngineApi>,
    request: wire::ExplainApiRequest,
) -> Result<wire::ExplainApiResponse, ConnectError> {
    let id = request.api;
    let written = api.blocking(move |engine| Handle::current().block_on(engine.written_api(&id))).await?;
    Ok(wire::ExplainApiResponse {
        path: written.path,
        pointer: written.pointer,
        digest: written.digest,
        api_json: written.json,
        ..wire::ExplainApiResponse::default()
    })
}

/// A call's draft as the engine takes it; an edit with no op is refused by its place.
pub(crate) fn draft(draft: &wire::ApiDraft) -> Result<CallDraft, ConnectError> {
    let edits = draft.edits.iter().enumerate().map(pointer_edit).collect::<Result<Vec<_>, _>>()?;
    Ok(CallDraft { path: draft.path.clone(), digest: draft.digest.clone(), edits })
}

#[cfg(test)]
#[path = "client_rpcs_tests.rs"]
mod tests;
