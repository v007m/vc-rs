//! The web UI's git pane over the project's version control (D546–D550): the branch and changed
//! files, a file's diff, and a commit, push or pull. Only the web UI calls these; no MCP tool or
//! agent can (D550). Diffs and what a push or pull said leave masked as a run's values do.

use vc_interfaces::{VcsCommit, VcsError, VcsStatus};
use vc_model::SourcePath;

use crate::{Engine, EngineError};

impl Engine {
    /// The branch, its upstream, how far apart they are, and the changed files (D546).
    ///
    /// # Errors
    ///
    /// [`EngineError::Vcs`] when there is no git or the project is in no repository.
    pub fn vcs_status(&self) -> Result<VcsStatus, EngineError> {
        self.vcs.status().map_err(|error| vcs_error(&error))
    }

    /// The diff of the file at `path`, relative to the project root, masked (D547).
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when `path` names no file in the project, and as
    /// [`Engine::vcs_status`].
    pub async fn vcs_diff(&self, path: &str) -> Result<String, EngineError> {
        let diff = self.vcs.diff(&file(path)?).map_err(|error| vcs_error(&error))?;
        Ok(self.masked_text(&diff).await)
    }

    /// Commits the files at `paths` as they are now, and only them; the new commit's short ID
    /// (D548).
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when a path names no file in the project, and [`EngineError::Vcs`]
    /// when git refuses the commit.
    pub fn vcs_commit(&self, (paths, message): (&[String], &str)) -> Result<String, EngineError> {
        let paths = paths.iter().map(|path| file(path)).collect::<Result<Vec<_>, _>>()?;
        let commit = VcsCommit { paths, message: message.to_owned() };
        self.vcs.commit(&commit).map_err(|error| vcs_error(&error))
    }

    /// Pushes the branch to its upstream with the user's own credentials; what git said, masked
    /// (D549).
    ///
    /// # Errors
    ///
    /// [`EngineError::Vcs`] when the push fails.
    pub async fn vcs_push(&self) -> Result<String, EngineError> {
        let said = self.vcs.push().map_err(|error| vcs_error(&error))?;
        Ok(self.masked_text(&said).await)
    }

    /// Fast-forwards the branch to its upstream with the user's own credentials; what git said,
    /// masked (D549).
    ///
    /// # Errors
    ///
    /// [`EngineError::Vcs`] when the pull fails or is no fast-forward.
    pub async fn vcs_pull(&self) -> Result<String, EngineError> {
        let said = self.vcs.pull().map_err(|error| vcs_error(&error))?;
        Ok(self.masked_text(&said).await)
    }
}

/// `path` as a project path, or an [`EngineError::Edit`] naming it.
fn file(path: &str) -> Result<SourcePath, EngineError> {
    SourcePath::new(path).map_err(|_| EngineError::Edit {
        reason: format!("`{path}` names no file in the project"),
        help: Some("give a path relative to the project root, such as `services/users/users.api.yaml`".to_owned()),
    })
}

/// `error` as the engine reports it, with what to do when the engine knows.
fn vcs_error(error: &VcsError) -> EngineError {
    let help = match error {
        VcsError::Unavailable { .. } => Some("install git and put it on the PATH".to_owned()),
        _ => None,
    };
    EngineError::Vcs { reason: error.to_string(), help }
}

#[cfg(test)]
#[path = "vcs_pane_tests.rs"]
mod tests;
