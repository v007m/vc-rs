use vc_compiler::{CompileCache, CompileInput, compile};
use vc_diagnostics::{Diagnostics, Summary};
use vc_interfaces::SourceFilter;
use vc_model::SourceFiles;

use crate::problems::add_problems;
use crate::selection::Selection;
use crate::{Engine, EngineError};

/// What `vc check` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckRequest {
    /// Report only on files at or under these paths (relative to the project root, `/`
    /// separators); every file when empty. The whole project is compiled either way, so
    /// references into other files are still checked.
    pub paths: Vec<String>,
}

/// What a check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// is reported as a V1012 error at its path.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project cannot be searched;
    /// [`EngineError::NoFilesMatch`] when a requested path selects no definition file.
    pub fn check(&self, request: &CheckRequest) -> Result<CheckReport, EngineError> {
        let found = self.sources.discover(&SourceFilter::default());
        let found = found.map_err(|error| EngineError::Sources { reason: error.to_string() })?;
        let mut notices = Vec::new();
        let previous = self.previous_cache(&mut notices);
        let compiled = compile(CompileInput { files: found.files, previous });
        if let Err(error) = self.compile_cache.save(&compiled.cache.to_bytes()) {
            notices.push(format!("the compile cache was not saved: {error}"));
        }
        let (mut diagnostics, mut files) = (compiled.diagnostics, compiled.files);
        add_problems(&found.problems, &mut diagnostics, &mut files);
        let selection = Selection::new(&request.paths, &files)?;
        let mut diagnostics = selection.keep(diagnostics, &files);
        diagnostics.sort();
        Ok(CheckReport { checked: selection.count(&files), diagnostics, files, notices })
    }

    fn previous_cache(&self, notices: &mut Vec<String>) -> CompileCache {
        match self.compile_cache.load() {
            Ok(Some(bytes)) => CompileCache::from_bytes(&bytes),
            Ok(None) => CompileCache::new(),
            Err(error) => {
                notices.push(format!("the compile cache was not used: {error}"));
                CompileCache::new()
            }
        }
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
