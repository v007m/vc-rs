//! Reads the project and compiles it, as the engine does for every command (`engine/project.rs`):
//! discovery with the config's `paths` globs, `.env.schema`, then the data and hook files the
//! compile wants, through the compiler's own loop ([`vc_compiler::compile_wanted`]).
//! There is no compile cache: each run compiles from scratch.

use std::sync::Arc;

use vc_compiler::{CompileCache, CompileInput, Compiled, SourceGlobs, Wanted, WantedRead};
use vc_diagnostics::Diagnostics;
use vc_interfaces::{Discovery, MemorySources, SourceError, SourceFilter, SourceProvider};
use vc_model::{Date, Program, SourceFiles, SourcePath};

use crate::BrowserError;

/// The project as compiled: the program when nothing stops one, every diagnostic and the files
/// they point into.
pub struct Project {
    pub program: Option<Arc<Program>>,
    pub diagnostics: Diagnostics,
    pub files: SourceFiles,
}

impl Project {
    /// The program, when the project has no errors.
    pub fn valid(self) -> Result<Arc<Program>, BrowserError> {
        match self.program {
            Some(program) if !self.diagnostics.has_errors() => Ok(program),
            _ => Err(BrowserError::invalid(self.diagnostics, self.files)),
        }
    }
}

/// Compiles `project` as of `today`, with every problem reading it as a V1012 diagnostic.
pub fn compile(project: &MemorySources, today: Date) -> Result<Project, BrowserError> {
    let mut found = discover(project)?;
    let secret_schema = vc_interfaces::secret_schema(project, &mut found);
    let input = CompileInput {
        files: found.files,
        previous: CompileCache::new(),
        today: Some(today),
        assets: Some(found.assets.into_iter().collect()),
        secret_schema,
    };
    let (compiled, unread) = compile_wanted(project, input)?;
    let (mut diagnostics, mut files) = (compiled.diagnostics, compiled.files);
    vc_interfaces::add_problems(&found.problems, &mut diagnostics, &mut files);
    vc_interfaces::add_problems(&unread, &mut diagnostics, &mut files);
    diagnostics.sort();
    Ok(Project { program: compiled.program, diagnostics, files })
}

/// The project's files, found again with the config's `paths` globs when it writes some.
fn discover(project: &MemorySources) -> Result<Discovery, BrowserError> {
    let found = project.discover(&SourceFilter::default()).map_err(sources_error)?;
    let globs = SourceGlobs::of(&found.files);
    if globs.is_default() {
        return Ok(found);
    }
    let filter = SourceFilter::written(&globs.include, globs.exclude.as_deref()).map_err(sources_error)?;
    project.discover(&filter).map_err(sources_error)
}

/// Compiles `input` through the compiler's loop, reading the files it wants from `project`;
/// the files that could not be read come back as problems.
///
/// The playground has no filesystem outside the project, so it drops `datasets.allowRoots`: a
/// data file outside the project is asked for like any other and is a file that cannot be read.
fn compile_wanted(project: &MemorySources, input: CompileInput) -> Result<(Compiled, Vec<SourceError>), BrowserError> {
    let read = |wanted: &[Wanted]| -> Result<WantedRead<SourceError>, BrowserError> {
        let paths: Vec<SourcePath> = wanted.iter().map(|(path, _)| path.clone()).collect();
        let found = project.read(&paths).map_err(sources_error)?;
        Ok((found.files, found.problems))
    };
    vc_compiler::compile_wanted(input, read)
        .map_err(|error| error.into_error(|reason| BrowserError::refused(reason, None)))
}

fn sources_error(error: impl std::fmt::Display) -> BrowserError {
    BrowserError::refused(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::Code;
    use vc_model::Date;

    use super::compile;
    use vc_interfaces::MemorySources;

    fn project(files: &[(&str, &str)]) -> MemorySources {
        MemorySources::from_files(files.iter().map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec())))
    }

    const CONFIG: &str = "syntaxVersion: 2\nproject: p\nprofiles:\n  - id: local\n    default: true\n";

    #[test]
    fn a_valid_project_compiles_to_a_program() {
        let api = "baseUrl: http://localhost:7421\napis:\n  - id: ping\n    path: /users\n";
        let files = project(&[("vibranium.config.yaml", CONFIG), ("services/ping/ping.api.yaml", api)]);
        let compiled = compile(&files, Date::UNIX_EPOCH).unwrap();
        assert!(!compiled.diagnostics.has_errors(), "{:?}", compiled.diagnostics);
        assert!(compiled.valid().is_ok());
    }

    #[test]
    fn a_missing_data_file_is_a_file_that_cannot_be_read() {
        let test = "tests:\n  - id: readsRows\n    examples: { $file: rows.json }\n    call: ping#ping\n";
        let api = "baseUrl: http://localhost:7421\napis:\n  - id: ping\n    path: /users\n";
        let files = project(&[
            ("vibranium.config.yaml", CONFIG),
            ("services/ping/ping.api.yaml", api),
            ("services/ping/rows.test.yaml", test),
        ]);
        let compiled = compile(&files, Date::UNIX_EPOCH).unwrap();
        let codes: Vec<Code> = compiled.diagnostics.iter().map(|found| found.code).collect();
        assert!(compiled.diagnostics.has_errors(), "{codes:?}");
        assert!(compiled.valid().is_err());
    }
}
