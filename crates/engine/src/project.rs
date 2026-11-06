//! Reads and compiles the project once per command: what `check`, `plan`, `list` and `run`
//! share.

use std::sync::Arc;

use vc_compiler::{CompileCache, CompileInput, SourceGlobs};
use vc_diagnostics::Diagnostics;
use vc_interfaces::{Discovery, SourceFilter};
use vc_model::{DEFAULT_SECRETS_SCHEMA, Program, SourceFile, SourceFiles, SourcePath};

use crate::edit::{self, FileChange};
use crate::{CheckReport, Engine, EngineError, problems};

/// The project as compiled: the program when nothing stops one, every diagnostic (discovery
/// problems included) and the files they point into.
pub struct Compiled {
    pub program: Option<Arc<Program>>,
    pub diagnostics: Diagnostics,
    pub files: SourceFiles,
    pub notices: Vec<String>,
}

/// A program without errors, and the compile's notices.
type Valid = (Arc<Program>, Vec<String>);

impl Engine {
    /// Reads the whole project and compiles it, through the compile cache.
    ///
    /// The compiler is given today's date from the clock, for expired skips (V3104), and the
    /// project's other files, for `$file` paths (V2004), and `.env.schema`, for the secret names
    /// (V2106, V2107). A definition file or schema that cannot be read is a V1012 error at its
    /// path; a cache that cannot be read or written only adds a notice.
    pub(crate) fn compiled(&self) -> Result<Compiled, EngineError> {
        self.compiled_with(&[])
    }

    /// As [`Engine::compiled`], with `changes` in place of the files they change; the cache is
    /// then read but not saved, since the files on disk did not change.
    pub(crate) fn compiled_with(&self, changes: &[FileChange]) -> Result<Compiled, EngineError> {
        let mut found = self.discover()?;
        let secret_schema = self.secret_schema(&mut found);
        let mut notices = Vec::new();
        let previous = self.previous_cache(&mut notices);
        edit::overlay(&mut found.files, changes)?;
        let input = CompileInput {
            files: found.files,
            previous: previous.clone(),
            today: Some(self.clock.today()),
            assets: Some(found.assets.into_iter().collect()),
            secret_schema,
        };
        let compiled = vc_compiler::compile(input);
        if changes.is_empty() {
            self.save_cache(&compiled.cache, &previous, &mut notices);
        }
        let (mut diagnostics, mut files) = (compiled.diagnostics, compiled.files);
        problems::add_problems(&found.problems, &mut diagnostics, &mut files);
        Ok(Compiled { program: compiled.program, diagnostics, files, notices })
    }

    /// The compiled program, for the commands that need one, and the compile's notices.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] with every diagnostic when any is an error, and the errors of
    /// [`Engine::compiled`].
    pub(crate) fn program(&self) -> Result<Valid, EngineError> {
        let compiled = self.compiled()?;
        match compiled.program {
            Some(program) if !compiled.diagnostics.has_errors() => Ok((program, compiled.notices)),
            _ => Err(invalid(compiled.diagnostics, compiled.files, compiled.notices)),
        }
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

    /// `.env.schema` at the project root, read when discovery listed it (D165). A schema that
    /// cannot be read joins `found`'s problems, so it is a V1012 error rather than no schema.
    fn secret_schema(&self, found: &mut Discovery) -> Option<SourceFile> {
        let path = SourcePath::new(DEFAULT_SECRETS_SCHEMA).ok()?;
        if !found.assets.contains(&path) {
            return None;
        }
        match self.sources.read(&[path]) {
            Ok(mut read) => {
                found.problems.append(&mut read.problems);
                read.files.pop()
            }
            Err(error) => {
                found.problems.push(error);
                None
            }
        }
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

/// The error that carries `diagnostics`, sorted, as a check report over every file.
pub fn invalid(diagnostics: Diagnostics, files: SourceFiles, notices: Vec<String>) -> EngineError {
    let mut diagnostics = diagnostics;
    diagnostics.sort();
    let checked = files.len();
    EngineError::Invalid(Box::new(CheckReport { diagnostics, files, checked, notices }))
}

pub fn sources_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Sources { reason: error.to_string() }
}
