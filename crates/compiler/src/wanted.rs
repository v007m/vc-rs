//! The compile loop (D172, D173): a compile names the data and hook files it was not given; the
//! caller reads them and the loop compiles again, until no file it has not asked for is wanted.
//! Pure: the reading is the closure the caller passes, so the engine reads the disk and the
//! docs playground its in-memory project through the same loop.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use vc_model::{SourceFile, SourcePath};

use crate::{CompileCache, CompileInput, Compiled, compile};

/// How many times wanted files are read and compiled again; a hook's import chain is read one
/// level per round.
const ROUNDS: usize = 8;

/// A file a compile wants: its path, and the `datasets.allowRoots` folder it must stay under
/// when it is outside the project (`None` for a file inside it).
pub type Wanted = (SourcePath, Option<SourcePath>);

/// What reading wanted files gave: the files read, and a problem for each one that could not be.
pub type WantedRead<P> = (Vec<SourceFile>, Vec<P>);

/// A settled compile, and the problems of the files that could not be read.
pub type Settled<P> = (Compiled, Vec<P>);

/// Why the loop stopped without a settled compile.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WantedError<E> {
    /// The reader failed outright.
    Read(E),
    /// The last compile still wants these files after every round.
    TooDeep(Vec<SourcePath>),
}

impl<E> WantedError<E> {
    /// The reader's error, or `too_deep` given the message for files still wanted.
    pub fn into_error(self, too_deep: impl FnOnce(String) -> E) -> E {
        match self {
            Self::Read(error) => error,
            Self::TooDeep(_) => too_deep(self.too_deep_message()),
        }
    }

    fn too_deep_message(&self) -> String {
        let Self::TooDeep(left) = self else { return String::new() };
        let paths: Vec<&str> = left.iter().map(SourcePath::as_str).collect();
        format!(
            "the project still names files after {ROUNDS} rounds of reading them: {}; a hook's imports may be nested too deep",
            paths.join(", ")
        )
    }
}

impl<E: fmt::Display> fmt::Display for WantedError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::TooDeep(_) => formatter.write_str(&self.too_deep_message()),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for WantedError<E> {}

/// The compile input as files are read into it.
struct Reading<P> {
    input: CompileInput,
    asked: BTreeSet<SourcePath>,
    problems: Vec<P>,
}

impl<P> Reading<P> {
    /// The files `compiled` wants that have not been asked for yet, in path order.
    fn fresh(&self, compiled: &Compiled) -> Vec<Wanted> {
        let data = compiled.data_wanted.iter().map(|data| (&data.path, data.root.as_ref()));
        let hooks = compiled.hooks_wanted.iter().map(|path| (path, None));
        let wanted: BTreeMap<&SourcePath, Option<&SourcePath>> = hooks.chain(data).collect();
        let fresh = wanted.into_iter().filter(|(path, _)| !self.asked.contains(*path));
        fresh.map(|(path, root)| (path.clone(), root.cloned())).collect()
    }

    /// Adds what reading `wanted` found, and the cache the next compile starts from.
    fn add(&mut self, wanted: Vec<Wanted>, (files, problems): WantedRead<P>, cache: CompileCache) {
        self.asked.extend(wanted.into_iter().map(|(path, _)| path));
        self.input.files.extend(files);
        self.problems.extend(problems);
        self.input.previous = cache;
    }
}

/// Compiles `input`, then has `read` read each file the compile wanted and compiles again.
///
/// It runs for at most eight rounds. A file asked for once is not asked for again, so one that
/// cannot be read ends the loop and comes back as one of the problems, next to the last compile.
///
/// # Errors
///
/// [`WantedError::Read`] when `read` fails, and [`WantedError::TooDeep`] when the last compile
/// still wants files no round has asked for.
pub fn compile_wanted<P, E>(
    input: CompileInput,
    mut read: impl FnMut(&[Wanted]) -> Result<WantedRead<P>, E>,
) -> Result<Settled<P>, WantedError<E>> {
    let mut reading = Reading { input, asked: BTreeSet::new(), problems: Vec::new() };
    let mut compiled = compile(reading.input.clone());
    for _ in 0..ROUNDS {
        let wanted = reading.fresh(&compiled);
        if wanted.is_empty() {
            return Ok((compiled, reading.problems));
        }
        let found = read(&wanted).map_err(WantedError::Read)?;
        reading.add(wanted, found, compiled.cache);
        compiled = compile(reading.input.clone());
    }
    let left = reading.fresh(&compiled);
    if left.is_empty() {
        Ok((compiled, reading.problems))
    } else {
        Err(WantedError::TooDeep(left.into_iter().map(|(path, _)| path).collect()))
    }
}

#[cfg(test)]
#[path = "wanted_tests.rs"]
mod tests;
