//! The compile loop: a compile names the data and hook files it was not given (D172, D173); the
//! engine reads them and compiles again, until no file it has not asked for is wanted.

use std::collections::BTreeSet;

use vc_compiler::{CompileCache, CompileInput, Compiled};
use vc_interfaces::{Discovery, SourceError};
use vc_model::SourcePath;

use crate::project::sources_error;
use crate::{Engine, EngineError};

/// How many times the engine reads wanted files and compiles again; a hook's import chain is
/// read one level per round.
const ROUNDS: usize = 8;

/// The last compile, and the wanted files that could not be read.
pub type Settled = (Compiled, Vec<SourceError>);

/// The compile input as files are read into it.
struct Reading {
    input: CompileInput,
    asked: BTreeSet<SourcePath>,
    problems: Vec<SourceError>,
}

impl Reading {
    /// The files `compiled` wants that have not been asked for yet, in path order.
    fn fresh(&self, compiled: &Compiled) -> Vec<SourcePath> {
        let data = compiled.data_wanted.iter().map(|data| &data.path);
        let wanted: BTreeSet<&SourcePath> = data.chain(&compiled.hooks_wanted).collect();
        wanted.into_iter().filter(|path| !self.asked.contains(*path)).cloned().collect()
    }

    /// Adds what reading `wanted` found, and the cache the next compile starts from.
    fn add(&mut self, wanted: Vec<SourcePath>, found: Discovery, cache: CompileCache) {
        self.asked.extend(wanted);
        self.input.files.extend(found.files);
        self.problems.extend(found.problems);
        self.input.previous = cache;
    }
}

impl Engine {
    /// Compiles `input`, then reads each file the compile wanted and compiles again, for at most
    /// eight rounds. A file asked for once is not asked for again, so one that cannot be read
    /// ends the loop and comes back as a problem.
    ///
    /// # Errors
    ///
    /// [`EngineError::Sources`] when the project root cannot be read, or when the last compile
    /// still wants files no round has asked for.
    pub(crate) fn compile_wanted(&self, input: CompileInput) -> Result<Settled, EngineError> {
        let mut reading = Reading { input, asked: BTreeSet::new(), problems: Vec::new() };
        let mut compiled = vc_compiler::compile(reading.input.clone());
        for _ in 0..ROUNDS {
            let wanted = reading.fresh(&compiled);
            if wanted.is_empty() {
                return Ok((compiled, reading.problems));
            }
            let found = self.sources.read(&wanted).map_err(sources_error)?;
            reading.add(wanted, found, compiled.cache);
            compiled = vc_compiler::compile(reading.input.clone());
        }
        let left = reading.fresh(&compiled);
        if left.is_empty() { Ok((compiled, reading.problems)) } else { Err(too_deep(&left)) }
    }
}

/// The error for files still wanted after the last round.
fn too_deep(left: &[SourcePath]) -> EngineError {
    let paths: Vec<&str> = left.iter().map(SourcePath::as_str).collect();
    let reason = format!(
        "the project still names files after {ROUNDS} rounds of reading them: {}; a hook's imports may be nested too deep",
        paths.join(", ")
    );
    EngineError::Sources { reason }
}

#[cfg(test)]
#[path = "wanted_tests.rs"]
mod tests;
