//! The git pane's RPCs (D546–D550): `VcsStatus`, `VcsDiff`, `VcsCommit`, `VcsPush` and
//! `VcsPull`. Only the browser's session reaches them; the MCP server offers no tool for them,
//! so no agent can commit, push or pull. A diff and what git said leave masked, and no message
//! holds a URL's credentials.

use std::sync::Arc;

use tokio::runtime::Handle;
use vc_contracts::engine::v1 as wire;
use vc_engine::{Engine, VcsChange, VcsFile};

use crate::ConnectError;
use crate::engine_api::{EngineApi, Rpcs};

/// Adds the `Vcs*` RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("VcsStatus", status)
        .unary("VcsDiff", diff)
        .unary("VcsCommit", commit)
        .unary("VcsPush", push)
        .unary("VcsPull", pull)
}

async fn status(api: Arc<EngineApi>, _: wire::VcsStatusRequest) -> Result<wire::VcsStatusResponse, ConnectError> {
    let status = api.blocking(Engine::vcs_status).await?;
    Ok(wire::VcsStatusResponse {
        branch: status.branch,
        upstream: status.upstream,
        ahead: status.ahead,
        behind: status.behind,
        files: status.files.iter().map(wire_file).collect(),
    })
}

async fn diff(api: Arc<EngineApi>, request: wire::VcsDiffRequest) -> Result<wire::VcsDiffResponse, ConnectError> {
    let path = request.path;
    let diff = api.blocking(move |engine| Handle::current().block_on(engine.vcs_diff(&path))).await?;
    Ok(wire::VcsDiffResponse { diff })
}

async fn commit(api: Arc<EngineApi>, request: wire::VcsCommitRequest) -> Result<wire::VcsCommitResponse, ConnectError> {
    let (paths, message) = (request.paths, request.message);
    let commit = api.blocking(move |engine| engine.vcs_commit((&paths, &message))).await?;
    Ok(wire::VcsCommitResponse { commit })
}

async fn push(api: Arc<EngineApi>, _: wire::VcsPushRequest) -> Result<wire::VcsPushResponse, ConnectError> {
    let output = api.blocking(|engine| Handle::current().block_on(engine.vcs_push())).await?;
    Ok(wire::VcsPushResponse { output })
}

async fn pull(api: Arc<EngineApi>, _: wire::VcsPullRequest) -> Result<wire::VcsPullResponse, ConnectError> {
    let output = api.blocking(|engine| Handle::current().block_on(engine.vcs_pull())).await?;
    Ok(wire::VcsPullResponse { output })
}

fn wire_file(file: &VcsFile) -> wire::VcsFile {
    let change = match file.change {
        VcsChange::Added => wire::VcsChange::Added,
        VcsChange::Modified => wire::VcsChange::Modified,
        VcsChange::Deleted => wire::VcsChange::Deleted,
        VcsChange::Untracked => wire::VcsChange::Untracked,
        VcsChange::Conflicted => wire::VcsChange::Conflicted,
    };
    let (added, removed, binary) = (file.added, file.removed, file.binary);
    wire::VcsFile { path: file.path.as_str().to_owned(), change: change.into(), added, removed, binary }
}

#[cfg(test)]
#[path = "vcs_rpcs_tests.rs"]
mod tests;
