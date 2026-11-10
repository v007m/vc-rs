//! The web UI editor's files (M9b): one project file's text with its digest, and a save that is
//! refused when the file changed since the editor opened it. Only the config and the api, test
//! and load files outside hidden folders are opened this way, so `.env`, `.git/`, `.vibranium/`
//! and the rest of the folder are never read or written through the UI, nor what a link leads
//! to outside the project or in a hidden folder. A save is not guarded
//! (D107): the editor keeps text with errors, and gets the project's diagnostics back.

use std::fmt::Write as _;

use vc_interfaces::{SourceError, content_hash};
use vc_model::{FileKind, SourceFile, SourcePath};

use crate::authoring::refused;
use crate::edit::{Changes, FileChange};
use crate::project::sources_error;
use crate::{CheckReport, CheckRequest, Engine, EngineError};

/// What can be done about a path the editor does not open.
const PATH_HELP: &str = "open vibranium.config.yaml or a project path ending in .api.yaml, .test.yaml or .load.yaml";

/// A project file as the editor opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpenedFile {
    /// Its whole text.
    pub text: String,
    /// Its BLAKE3 hash in lowercase hex, which a save gives back to show what it changed.
    pub digest: String,
}

/// A save from the editor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct SaveFile {
    /// Where, relative to the project root: `services/users/crud.test.yaml`.
    pub path: String,
    /// The file's whole new text.
    pub text: String,
    /// The digest the file had when it was opened; `None` for a file that does not exist yet.
    pub expected_digest: Option<String>,
}

/// What a save wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SavedFile {
    /// The digest of the text written.
    pub digest: String,
    /// The project's diagnostics with the file saved, or why it could not be checked; the file
    /// was written either way.
    pub check: Result<CheckReport, EngineError>,
}

impl Engine {
    /// The text of the file at `path`, with its digest.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] when the path is not the config or an api, test or load file
    /// outside a hidden folder, or links outside the project or to a hidden file; [`EngineError::NoFilesMatch`] when no file there can be read.
    pub fn read_file(&self, path: &str) -> Result<OpenedFile, EngineError> {
        let path = editable(path)?;
        let file = self.current(&path)?.ok_or_else(|| EngineError::NoFilesMatch { path: path.to_string() })?;
        Ok(OpenedFile { digest: hex(file.hash.as_bytes()), text: file.text })
    }

    /// Writes `request.text` to `request.path` when the file is as the editor opened it, then
    /// checks the project. Once the file is written, its digest comes back even when the check
    /// fails, so the next save is not stale.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] as [`Engine::read_file`]; [`EngineError::Stale`] when the file
    /// changed, appeared or was removed since it was opened; [`EngineError::Sources`] when it
    /// cannot be written.
    pub fn save_file(&self, request: &SaveFile) -> Result<SavedFile, EngineError> {
        let path = editable(&request.path)?;
        let before = self.current(&path)?;
        fresh(&path, before.as_ref(), request.expected_digest.as_deref())?;
        let (after, before) = (request.text.clone(), before.map(|file| file.text));
        let change = FileChange { path: path.to_string(), before, after, moved_from: None };
        self.write(&Changes { files: vec![change], titles: Vec::new() })?;
        let check = self.check(&CheckRequest::default());
        Ok(SavedFile { digest: hex(content_hash(request.text.as_bytes()).as_bytes()), check })
    }

    /// The file at `path` as it is now; `None` when no file there can be read.
    fn current(&self, path: &SourcePath) -> Result<Option<SourceFile>, EngineError> {
        let mut read = self.sources.read(std::slice::from_ref(path)).map_err(sources_error)?;
        match read.problems.pop() {
            None => Ok(read.files.pop()),
            Some(SourceError::Read { .. }) => Ok(None),
            Some(SourceError::OutsideRoot { .. }) => {
                Err(refused(format!("`{path}` links to a file outside the project or a hidden one"), PATH_HELP))
            }
            Some(problem) => Err(sources_error(problem)),
        }
    }
}

/// `text` as a project path the editor opens.
fn editable(text: &str) -> Result<SourcePath, EngineError> {
    let path =
        SourcePath::new(text).map_err(|_| refused(format!("`{text}` is not a path in the project"), PATH_HELP))?;
    let hidden = path.as_str().split('/').any(|part| part.starts_with('.'));
    match FileKind::from_path(&path) {
        Some(_) if !hidden => Ok(path),
        _ => Err(refused(format!("`{text}` is not a definition file the editor opens"), PATH_HELP)),
    }
}

/// Whether the file at `path`, `before`, is the one the editor opened with `expected`.
fn fresh(path: &SourcePath, before: Option<&SourceFile>, expected: Option<&str>) -> Result<(), EngineError> {
    let reason = match (before, expected) {
        (None, None) => return Ok(()),
        (Some(file), Some(digest)) if hex(file.hash.as_bytes()) == digest => return Ok(()),
        (Some(_), Some(_)) => "it changed since it was opened",
        (Some(_), None) => "it exists already",
        (None, Some(_)) => "it was removed since it was opened",
    };
    Err(EngineError::Stale { reason: format!("{path}: nothing was written: {reason}") })
}

/// `bytes` in lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(test)]
#[path = "editor_files_tests.rs"]
mod tests;
