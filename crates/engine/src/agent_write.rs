//! `write_definition`: an agent writes one api, test or load file (D195). The text passes the
//! compile guard (D107) first, then the profiles' `writeDefinitions` rule, so nothing that would
//! break the project is ever put to a person for approval, and nothing is written without it.
//! The config is never written this way: it holds the very rules that judge the agent. A write
//! that fails before the gate is logged as `failed: <reason>` (D204).

use serde_json::json;
use vc_model::{FileKind, SourcePath};

use crate::authoring::{Authoring, refused};
use crate::edit::{Changes, FileChange};
use crate::policy::AgentCall;
use crate::{Engine, EngineError};

/// The tool an agent's write is judged and logged as.
pub const WRITE_TOOL: &str = "write_definition";

/// What can be done about a path that is not a definition file.
const PATH_HELP: &str = "give a project path ending in .api.yaml, .test.yaml or .load.yaml";

/// One definition file an agent writes whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct WriteRequest {
    /// Where, relative to the project root: `services/users/crud.test.yaml`.
    pub path: String,
    /// The file's whole new text.
    pub text: String,
    /// The agent writing it.
    pub agent: String,
    /// `--profile`: the profiles whose `agents` rules judge the write.
    pub profiles: Vec<String>,
}

impl Engine {
    /// Writes `request.text` to `request.path` once the compile guard passes it and the
    /// profiles' `writeDefinitions` rule allows it or a person approves; the change written.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the path is not an api, test or load file in the project;
    /// as the compile guard when the text would add an error; [`EngineError::Refused`] when the
    /// policy refuses it or no one approves it in time; [`EngineError::Sources`] when it cannot
    /// be written.
    pub async fn write_definition(&self, request: &WriteRequest) -> Result<Changes, EngineError> {
        let call = AgentCall {
            agent: request.agent.clone(),
            tool: WRITE_TOOL.to_owned(),
            profiles: request.profiles.clone(),
            writes: true,
            summary: format!("write {}", request.path),
            arguments: json!({"path": request.path, "text": request.text}),
            ..AgentCall::default()
        };
        let changes = self.before_gate(Some(&call), self.guarded_write(request))?;
        let gated = self.gated(call).await?;
        let written = self.write(&changes);
        self.settle(Some(gated), written.is_ok())?;
        written.map(|()| changes)
    }

    /// The change `request` makes, once the path is a definition file and the compile guard
    /// passes it.
    fn guarded_write(&self, request: &WriteRequest) -> Result<Changes, EngineError> {
        let path = definition_path(&request.path)?;
        let project = self.authoring()?;
        self.guarded(&project, changes(&project, &path, &request.text))
    }
}

/// `text` as a project path to an api, test or load file.
fn definition_path(text: &str) -> Result<String, EngineError> {
    let path =
        SourcePath::new(text).map_err(|_| refused(format!("`{text}` is not a path in the project"), PATH_HELP))?;
    match FileKind::from_path(&path) {
        Some(FileKind::Api | FileKind::Test | FileKind::Load) => Ok(path.as_str().to_owned()),
        Some(FileKind::Config) => {
            Err(refused("an agent cannot write vibranium.config.yaml", "it holds the agent rules; a person edits it"))
        }
        None => Err(refused(format!("`{text}` is not a definition file"), PATH_HELP)),
    }
}

/// The change that makes the file at `path` read `text`.
fn changes(project: &Authoring, path: &str, text: &str) -> Changes {
    let before = project.entries().find(|(_, file)| file.path.as_str() == path).map(|(_, file)| file.text.clone());
    let title = if before.is_some() { format!("Write {path}") } else { format!("Create {path}") };
    let after = text.to_owned();
    Changes { files: vec![FileChange { path: path.to_owned(), before, after, moved_from: None }], titles: vec![title] }
}

#[cfg(test)]
#[path = "agent_write_tests.rs"]
mod tests;
