//! The compile loop: a compile names the data and hook files it was not given (D172, D173); the
//! engine reads them and compiles again, until no file it has not asked for is wanted. The loop
//! itself is [`vc_compiler::compile_wanted`]; the engine supplies the reading.

use vc_compiler::{CompileInput, Wanted, WantedRead};
use vc_interfaces::{Discovery, SourceError};
use vc_model::SourcePath;

use crate::project::sources_error;
use crate::{Engine, EngineError};

/// The last compile, and the wanted files that could not be read.
pub type Settled = vc_compiler::Settled<SourceError>;

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
        vc_compiler::compile_wanted(input, |wanted| self.read_wanted(wanted))
            .map_err(|error| error.into_error(|reason| EngineError::Sources { reason }))
    }

    /// Reads `wanted`: the files inside the project as any file, and the ones outside it only
    /// under their `datasets.allowRoots` folder (D173).
    fn read_wanted(&self, wanted: &[Wanted]) -> Result<WantedRead<SourceError>, EngineError> {
        let inside: Vec<SourcePath> =
            wanted.iter().filter(|(_, root)| root.is_none()).map(|(path, _)| path.clone()).collect();
        let outside: Vec<(SourcePath, SourcePath)> =
            wanted.iter().filter_map(|(path, root)| root.clone().map(|root| (path.clone(), root))).collect();
        let mut found: Discovery = self.sources.read(&inside).map_err(sources_error)?;
        if !outside.is_empty() {
            let more = self.sources.read_outside(&outside).map_err(sources_error)?;
            found.files.extend(more.files);
            found.problems.extend(more.problems);
            found.sort();
        }
        Ok((found.files, found.problems))
    }
}

#[cfg(test)]
#[path = "wanted_tests.rs"]
mod tests;
