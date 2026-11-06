//! The words shell completion offers for the hidden `vc __complete` (D180): the project's file,
//! test and api IDs, its profiles and its tags.

use vc_model::{Program, Sym};

use crate::Engine;

/// What kind of word is being completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompleteKind {
    /// Test file IDs, and test IDs once the word has a `#`.
    Tests,
    /// Api file IDs, and api IDs once the word has a `#`.
    Apis,
    /// Both, for `vc explain`.
    Ids,
    /// Profile IDs; a comma list completes its last entry.
    Profiles,
    /// Tags; a comma list completes its last entry.
    Tags,
}

impl Engine {
    /// The words of `kind` that start with `word`, each once, in project order. Nothing when
    /// the project cannot be compiled: completion never prints an error.
    #[must_use]
    pub fn complete(&self, kind: CompleteKind, word: &str) -> Vec<String> {
        let Ok((program, _)) = self.program() else {
            return Vec::new();
        };
        let (before, last) = match kind {
            CompleteKind::Profiles | CompleteKind::Tags => word.rsplit_once(',').map_or(("", word), |(b, l)| (b, l)),
            CompleteKind::Tests | CompleteKind::Apis | CompleteKind::Ids => ("", word),
        };
        let mut words: Vec<String> = Vec::new();
        let starting = candidates(&program, kind, word.contains('#')).into_iter().filter(|c| c.starts_with(last));
        for candidate in starting {
            let full = if before.is_empty() { candidate } else { format!("{before},{candidate}") };
            if !words.contains(&full) {
                words.push(full);
            }
        }
        words
    }
}

/// Every word of `kind`; the IDs inside files only when `inside` (the word has a `#`).
fn candidates(program: &Program, kind: CompleteKind, inside: bool) -> Vec<String> {
    let text = |sym: Sym| program.str(sym).unwrap_or_default().to_owned();
    let tests = || {
        let files = program.test_files.keys().map(|id| text(id.sym()));
        let tests = program.tests.keys().filter(|_| inside).map(|id| text(id.sym()));
        files.chain(tests).collect::<Vec<_>>()
    };
    let apis = || {
        let files = program.api_groups.keys().map(|id| text(id.sym()));
        let apis = program.apis.keys().filter(|_| inside).map(|id| text(id.sym()));
        files.chain(apis).collect::<Vec<_>>()
    };
    match kind {
        CompleteKind::Tests => tests(),
        CompleteKind::Apis => apis(),
        CompleteKind::Ids => [tests(), apis()].concat(),
        CompleteKind::Profiles => program.profiles.keys().map(|id| text(id.sym())).collect(),
        CompleteKind::Tags => tags(program),
    }
}

/// The tags of every test file and test.
fn tags(program: &Program) -> Vec<String> {
    let files = program.test_files.values().flat_map(|file| &file.tags);
    let tests = program.tests.values().flat_map(|test| &test.tags);
    files.chain(tests).map(|tag| tag.0.clone()).collect()
}

#[cfg(test)]
#[path = "complete_tests.rs"]
mod tests;
