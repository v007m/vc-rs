use std::collections::BTreeMap;

use vc_diagnostics::{Diagnostics, Summary};
use vc_model::SourceFiles;

use crate::check_alerts;
use crate::edit::FileChange;
use crate::project::Compiled;
use crate::selection::Selection;
use crate::{Engine, EngineError};

/// What `vc check` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct CheckRequest {
    /// Report only on files at or under these paths (relative to the project root, `/`
    /// separators); every file when empty. The whole project is compiled either way, so
    /// references into other files are still checked.
    pub paths: Vec<String>,
    /// Unsaved text by project-relative path, compiled in place of the file on disk (or as a
    /// new file); the compile cache is then read but not saved.
    pub overlay: BTreeMap<String, String>,
}

/// What a check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CheckReport {
    /// The diagnostics on the selected files, in display order.
    pub diagnostics: Diagnostics,
    /// The files the diagnostics point into.
    pub files: SourceFiles,
    /// How many files were selected.
    pub checked: usize,
    /// Problems that did not affect the result, such as a compile cache that could not be
    /// read or written; one line each.
    pub notices: Vec<String>,
}

impl CheckReport {
    /// The count of each severity.
    #[must_use]
    pub fn summary(&self) -> Summary {
        self.diagnostics.summary()
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}

impl Engine {
    /// Reads and compiles the whole project and reports the diagnostics on the selected files.
    ///
    /// Uses the compile cache the previous check left, and leaves one for the next. A cache
    /// that cannot be read or written only adds a notice. A definition file that cannot be read
    /// is reported as a V1012 error at its path. The compiler is given today's date from the
    /// clock, for expired skips (V3104), and the project's other files, for `$file` paths (V2004).
    /// An `alerts` channel no notifier builds is a V5117 warning.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project cannot be searched or a `paths` glob does not
    /// parse;
    /// [`EngineError::NoFilesMatch`] when a requested path selects no definition file.
    pub fn check(&self, request: &CheckRequest) -> Result<CheckReport, EngineError> {
        let overlay: Vec<_> = request.overlay.iter().map(|(path, text)| unsaved(path, text)).collect();
        let mut compiled = self.compiled_with(&overlay)?;
        compiled.diagnostics.extend(check_alerts::unknown_channels(&compiled.files, &self.channels()));
        Self::selected(compiled, request)
    }

    /// The diagnostics of `compiled` on the files `request` selects.
    pub(crate) fn selected(compiled: Compiled, request: &CheckRequest) -> Result<CheckReport, EngineError> {
        let (diagnostics, files) = (compiled.diagnostics, compiled.files);
        let selection = Selection::new(&request.paths, &files)?;
        let mut diagnostics = selection.keep(diagnostics, &files);
        diagnostics.sort();
        Ok(CheckReport { checked: selection.count(&files), diagnostics, files, notices: compiled.notices })
    }
}

/// `text` in the editor at `path`, as a change to compile in place of the file.
fn unsaved(path: &str, text: &str) -> FileChange {
    FileChange { path: path.to_owned(), before: None, after: text.to_owned(), moved_from: None }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
