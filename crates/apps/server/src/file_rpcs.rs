//! The editor's file operations (ui.md §10 row 6): `CreateFile` from `vc new file`'s template
//! and `Rename` as `vc rename`, each previewed with every file it changes and written only with
//! the preview's digest, as a skip is; `Format`, the editor's unsaved text in the house style;
//! and `FileReferences` and `TrashFile`, the confirmation list and the move into the project's
//! trash (D683, D684). Each reads the project on the blocking pool.

use std::sync::Arc;

use vc_contracts::engine::v1 as wire;
use vc_engine::{FileKind, FileReference};

use crate::authoring_rpcs::{previewed, response};
use crate::engine_api::{EngineApi, Rpcs};
use crate::{Code, ConnectError};

/// The kinds of file `CreateFile` makes, by the name a request gives.
const KINDS: [(&str, FileKind); 3] = [("api", FileKind::Api), ("test", FileKind::Test), ("load", FileKind::Load)];

/// Adds the editor's file operations.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("CreateFile", create_file)
        .unary("Rename", rename)
        .unary("Format", format)
        .unary("FileReferences", file_references)
        .unary("TrashFile", trash_file)
}

/// `vc new file <kind> <id>` for the web UI: the new file previewed, or written with the
/// preview's digest; an existing file is never overwritten.
async fn create_file(
    api: Arc<EngineApi>,
    request: wire::CreateFileRequest,
) -> Result<wire::CreateFileResponse, ConnectError> {
    let kind = KINDS.iter().find(|(name, _)| *name == request.kind).map(|&(_, kind)| kind);
    let kind = kind.ok_or_else(|| ConnectError::new(Code::InvalidArgument, "kind must be api, test or load"))?;
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let id = request.id;
    let (path, changes, digest) = api
        .blocking(move |engine| {
            let added = engine.new_file(kind, &id)?;
            let (changes, digest) = previewed(engine, added.changes, expected)?;
            Ok((added.path, changes, digest))
        })
        .await?;
    let wire::ImportResponse { files, titles } = response(changes);
    Ok(wire::CreateFileResponse { files, titles, digest, path })
}

/// `vc rename <from> <to>` for the web UI: every file it changes previewed, or written with the
/// preview's digest.
async fn rename(api: Arc<EngineApi>, request: wire::RenameRequest) -> Result<wire::RenameResponse, ConnectError> {
    let expected = request.write.then(|| request.expected_digest.unwrap_or_default());
    let (from, to) = (request.from, request.to);
    let (changes, digest) = api.blocking(move |engine| previewed(engine, engine.rename(&from, &to)?, expected)).await?;
    let wire::ImportResponse { files, titles } = response(changes);
    Ok(wire::RenameResponse { files, titles, digest })
}

/// `vc fmt` on the editor's text, which the editor puts in place unsaved (D684).
async fn format(api: Arc<EngineApi>, request: wire::FormatRequest) -> Result<wire::FormatResponse, ConnectError> {
    let wire::FormatRequest { path, text, strip_comments } = request;
    let before = text.clone();
    let text = api.blocking(move |engine| engine.format_text(&path, &text, strip_comments)).await?;
    Ok(wire::FormatResponse { changed: text != before, text })
}

async fn file_references(
    api: Arc<EngineApi>,
    request: wire::FileReferencesRequest,
) -> Result<wire::FileReferencesResponse, ConnectError> {
    let path = request.path;
    let found = api.blocking(move |engine| engine.file_references(&path)).await?;
    Ok(wire::FileReferencesResponse {
        references: found.references.into_iter().map(reference).collect(),
        unread: found.unread,
    })
}

/// Sends the file to the system's trash when it is the one the editor read (D683).
async fn trash_file(
    api: Arc<EngineApi>,
    request: wire::TrashFileRequest,
) -> Result<wire::TrashFileResponse, ConnectError> {
    let wire::TrashFileRequest { path, expected_digest } = request;
    let trashed_to = api.blocking(move |engine| engine.trash_file(&path, &expected_digest)).await?;
    Ok(wire::TrashFileResponse { trashed_to })
}

fn reference(found: FileReference) -> wire::FileReference {
    wire::FileReference { path: found.path, line: found.line, from: found.from, target: found.target }
}

#[cfg(test)]
#[path = "file_rpcs_tests.rs"]
mod tests;
