use vc_diagnostics::{Diagnostics, Summary};
use vc_model::SourceFiles;

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
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project cannot be searched or a `paths` glob does not
    /// parse;
    /// [`EngineError::NoFilesMatch`] when a requested path selects no definition file.
    pub fn check(&self, request: &CheckRequest) -> Result<CheckReport, EngineError> {
        let compiled = self.compiled()?;
        let (diagnostics, files) = (compiled.diagnostics, compiled.files);
        let selection = Selection::new(&request.paths, &files)?;
        let mut diagnostics = selection.keep(diagnostics, &files);
        diagnostics.sort();
        Ok(CheckReport { checked: selection.count(&files), diagnostics, files, notices: compiled.notices })
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
