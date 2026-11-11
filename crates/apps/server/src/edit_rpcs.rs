//! The form views' `EditFile`: values set, removed or added by JSON pointer in one config, api,
//! test or load file, previewed or written, over `Engine::edit_by_pointer`. Its diff leaves
//! masked, and a refusal names the edit by its place, never its pointer or value.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::{EditOp, PointerEdit, PointerEdits};

use crate::engine_api::{EngineApi, Rpcs, message};
use crate::wire_diagnostics::diagnostics;
use crate::{Code, ConnectError};

/// Adds `EditFile`.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("EditFile", edit_file)
}

async fn edit_file(
    api: Arc<EngineApi>,
    request: wire::EditFileRequest,
) -> Result<wire::EditFileResponse, ConnectError> {
    let edits = request.edits.iter().enumerate().map(pointer_edit).collect::<Result<Vec<_>, _>>()?;
    let edits = PointerEdits { path: request.path, digest: request.digest, edits, write: request.write };
    let edited = api.blocking(move |engine| Handle::current().block_on(engine.edit_by_pointer(&edits))).await?;
    let (diagnostics, check_error) = match &edited.check {
        Some(Ok(report)) => (diagnostics(report), String::new()),
        Some(Err(error)) => (Vec::new(), message(error)),
        None => (Vec::new(), String::new()),
    };
    let written = edited.check.is_some();
    Ok(wire::EditFileResponse { diff: edited.diff, digest: edited.digest, written, diagnostics, check_error })
}

/// The `at`th edit as the engine takes it; one with no op is refused by its place.
fn pointer_edit((at, edit): (usize, &wire::PointerEdit)) -> Result<PointerEdit, ConnectError> {
    let op = match edit.op() {
        wire::EditOp::Set => EditOp::Set,
        wire::EditOp::Remove => EditOp::Remove,
        wire::EditOp::Insert => EditOp::Insert,
        wire::EditOp::Unspecified => {
            let reason = format!("edit {} needs an op: set, remove or insert", at + 1);
            return Err(ConnectError::new(Code::InvalidArgument, reason));
        }
    };
    Ok(PointerEdit { pointer: edit.pointer.clone(), op, value: edit.value_json.clone() })
}

#[cfg(test)]
#[path = "edit_rpcs_tests.rs"]
mod tests;
