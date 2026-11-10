//! The web UI editor's files (M9b): one project file's text with its digest, and a save that is
//! refused when the file changed since the editor opened it. Only the config and the api, test
//! and load files outside hidden folders are opened this way, so `.env`, `.git/`, `.vibranium/`
//! and the rest of the folder are never read or written through the UI, nor what a link leads
//! to outside the project or in a hidden folder, nor a path discovery leaves out (`paths`
//! globs, `node_modules/` by default, ignore files). A save is not guarded
//! (D107): the editor keeps text with errors, and gets the project's diagnostics back.
//!
//! A save compares the digest when it reads the file, and the writer compares the hash again
//! just before it replaces the file ([`vc_interfaces::SourceWriter::write_unchanged`]). No lock
//! is taken (locks are only for the token cache and the redactor), so a change another writer
//! makes between that second check and the replace is still lost.

use std::fmt::Write as _;

use vc_interfaces::{SourceError, content_hash};
use vc_model::{FileKind, SourceFile, SourcePath};

use crate::authoring::refused;
use crate::edit::internal;
use crate::project::sources_error;
use crate::{CheckReport, CheckRequest, Engine, EngineError};

/// What can be done about a path the editor does not open.
const PATH_HELP: &str = "open vibranium.config.yaml or a project path ending in .api.yaml, .test.yaml or .load.yaml";

/// What can be done about a path discovery leaves out.
const LEFT_OUT_HELP: &str =
    "pick a path the project reads, or change `paths` in vibranium.config.yaml or the ignore file that leaves it out";

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
    /// outside a hidden folder, discovery leaves it out, or it links outside the project or to a
    /// hidden file; [`EngineError::NoFilesMatch`] when there is no file there;
    /// [`EngineError::Sources`] when it cannot be read or is not UTF-8, or the config's `paths`
    /// globs do not parse.
    pub fn read_file(&self, path: &str) -> Result<OpenedFile, EngineError> {
        let path = self.editable(path)?;
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
        let path = self.editable(&request.path)?;
        let before = self.current(&path)?;
        fresh(&path, before.as_ref(), request.expected_digest.as_deref())?;
        let writer = self.writer.as_ref().ok_or_else(|| internal("this engine has no folder to write into"))?;
        writer.write_unchanged(&path, &request.text, before.map(|file| file.hash)).map_err(|error| match error {
            SourceError::Changed { .. } => stale(&path, "it changed since it was opened"),
            other => sources_error(other),
        })?;
        let check = self.check(&CheckRequest::default());
        Ok(SavedFile { digest: hex(content_hash(request.text.as_bytes()).as_bytes()), check })
    }

    /// `text` as a path the editor reads and writes: the config or an api, test or load file
    /// outside a hidden folder that discovery reads, that is not a link out of the project or to
    /// a hidden file and that can be read when it is there. A file that is not there yet is a path the editor
    /// may create.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] and [`EngineError::Sources`] as [`Engine::read_file`].
    pub(crate) fn editor_path(&self, text: &str) -> Result<SourcePath, EngineError> {
        let path = self.editable(text)?;
        self.current(&path)?;
        Ok(path)
    }

    /// The file at `path` as it is now; `None` when there is no file there. A file that is there
    /// but cannot be read, or is not UTF-8, is an error, so a save never takes it for a new file.
    fn current(&self, path: &SourcePath) -> Result<Option<SourceFile>, EngineError> {
        let mut read = self.sources.read(std::slice::from_ref(path)).map_err(sources_error)?;
        match read.problems.pop() {
            None => Ok(read.files.pop()),
            Some(SourceError::Missing { .. }) => Ok(None),
            Some(SourceError::OutsideRoot { .. }) => {
                Err(refused(format!("`{path}` links to a file outside the project or a hidden one"), PATH_HELP))
            }
            Some(problem) => Err(sources_error(problem)),
        }
    }

    /// `text` as a definition path the editor opens ([`definition`]) that discovery would read:
    /// the config's `paths` globs and the project's ignore files keep it in. The config is
    /// always read, so it opens even when its globs do not parse.
    fn editable(&self, text: &str) -> Result<SourcePath, EngineError> {
        let path = definition(text)?;
        let config = FileKind::from_path(&path) == Some(FileKind::Config);
        if config || (self.source_filter()?.allows(&path) && !self.sources.ignores(&path)) {
            return Ok(path);
        }
        Err(refused(format!("`{path}` is a path the project leaves out of discovery"), LEFT_OUT_HELP))
    }
}

/// `text` as a project path naming the config or an api, test or load file outside a hidden
/// folder.
fn definition(text: &str) -> Result<SourcePath, EngineError> {
    let path =
        SourcePath::new(text).map_err(|_| refused(format!("`{text}` is not a path in the project"), PATH_HELP))?;
    match FileKind::from_path(&path) {
        Some(_) if !path.is_hidden() => Ok(path),
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
    Err(stale(path, reason))
}

/// The refusal of a save over `path`, which is not the file the editor opened, for `reason`.
fn stale(path: &SourcePath, reason: &str) -> EngineError {
    EngineError::Stale { reason: format!("{path}: nothing was written: {reason}") }
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
