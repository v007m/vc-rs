use vc_compiler::{CompileCache, CompileInput, SourceGlobs};
use vc_diagnostics::{Diagnostics, Summary};
use vc_interfaces::{Discovery, SourceFilter};
use vc_model::SourceFiles;

use crate::problems;
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
    /// is reported as a V1012 error at its path. The compiler is given today's date from the
    /// clock, for expired skips (V3104), and the project's other files, for `$file` paths (V2004).
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project cannot be searched or a `paths` glob does not
    /// parse;
    /// [`EngineError::NoFilesMatch`] when a requested path selects no definition file.
    pub fn check(&self, request: &CheckRequest) -> Result<CheckReport, EngineError> {
        let found = self.discover()?;
        let mut notices = Vec::new();
        let previous = self.previous_cache(&mut notices);
        let input = CompileInput {
            files: found.files,
            previous: previous.clone(),
            today: Some(self.clock.today()),
            assets: Some(found.assets.into_iter().collect()),
        };
        let compiled = vc_compiler::compile(input);
        self.save_cache(&compiled.cache, &previous, &mut notices);
        let (mut diagnostics, mut files) = (compiled.diagnostics, compiled.files);
        problems::add_problems(&found.problems, &mut diagnostics, &mut files);
        let selection = Selection::new(&request.paths, &files)?;
        let mut diagnostics = selection.keep(diagnostics, &files);
        diagnostics.sort();
        Ok(CheckReport { checked: selection.count(&files), diagnostics, files, notices })
    }

    /// The project's files, found with the config's `paths` globs: discovery runs again with
    /// them only when the config writes some.
    fn discover(&self) -> Result<Discovery, EngineError> {
        let found = self.walk(&SourceFilter::default())?;
        let globs = SourceGlobs::of(&found.files);
        if globs.is_default() {
            return Ok(found);
        }
        let filter = SourceFilter::written(&globs.include, globs.exclude.as_deref()).map_err(sources_error)?;
        self.walk(&filter)
    }

    fn walk(&self, filter: &SourceFilter) -> Result<Discovery, EngineError> {
        self.sources.discover(filter).map_err(sources_error)
    }

    /// Saves `cache` unless it remembers the same files as `previous`, which the store holds.
    fn save_cache(&self, cache: &CompileCache, previous: &CompileCache, notices: &mut Vec<String>) {
        if cache == previous {
            return;
        }
        if let Err(error) = self.compile_cache.save(&cache.to_bytes()) {
            notices.push(format!("the compile cache was not saved: {error}"));
        }
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

fn sources_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Sources { reason: error.to_string() }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
