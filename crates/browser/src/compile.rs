//! Reads the project and compiles it, as the engine does for every command (`engine/project.rs`
//! and `engine/wanted.rs`): discovery with the config's `paths` globs, `.env.schema`, then the
//! data and hook files the compile wants, read and compiled again for at most eight rounds.
//! There is no compile cache: each run compiles from scratch.

use std::collections::BTreeSet;
use std::sync::Arc;

use vc_compiler::{CompileCache, CompileInput, Compiled, SourceGlobs};
use vc_diagnostics::Diagnostics;
use vc_interfaces::{Discovery, MemorySources, SourceError, SourceFilter, SourceProvider};
use vc_model::{Date, Program, SourceFiles, SourcePath};

use crate::BrowserError;

/// How many times wanted files are read and compiled again, as in the engine.
const ROUNDS: usize = 8;

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

/// Compiles `input`, then reads the files the compile wants that were not asked for yet and
/// compiles again; the files that could not be read come back as problems.
fn compile_wanted(
    project: &MemorySources,
    mut input: CompileInput,
) -> Result<(Compiled, Vec<SourceError>), BrowserError> {
    let (mut asked, mut problems) = (BTreeSet::new(), Vec::new());
    let mut compiled = vc_compiler::compile(input.clone());
    for _ in 0..ROUNDS {
        let wanted = fresh(&compiled, &asked);
        if wanted.is_empty() {
            return Ok((compiled, problems));
        }
        let found = project.read(&wanted).map_err(sources_error)?;
        asked.extend(wanted);
        input.files.extend(found.files);
        problems.extend(found.problems);
        input.previous = compiled.cache;
        compiled = vc_compiler::compile(input.clone());
    }
    let left = fresh(&compiled, &asked);
    if left.is_empty() { Ok((compiled, problems)) } else { Err(too_deep(&left)) }
}

/// The data and hook files `compiled` wants that were not asked for, in path order. A data file
/// outside the project is read like any other, so it is a file that cannot be read.
fn fresh(compiled: &Compiled, asked: &BTreeSet<SourcePath>) -> Vec<SourcePath> {
    let data = compiled.data_wanted.iter().map(|data| &data.path);
    let wanted: BTreeSet<&SourcePath> = compiled.hooks_wanted.iter().chain(data).collect();
    wanted.into_iter().filter(|path| !asked.contains(*path)).cloned().collect()
}

fn too_deep(left: &[SourcePath]) -> BrowserError {
    let paths: Vec<&str> = left.iter().map(SourcePath::as_str).collect();
    let reason = format!(
        "the project still names files after {ROUNDS} rounds of reading them: {}; a hook's imports may be nested too deep",
        paths.join(", ")
    );
    BrowserError::refused(reason, None)
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
