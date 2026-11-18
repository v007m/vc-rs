//! The link memo (D333): what the last clean compile produced, under everything it read besides
//! the files, so a compile of the same files and the same world does not parse or link again.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vc_diagnostics::Diagnostics;
use vc_model::{Date, Program, SourceFile, SourceFiles, SourcePath};

use super::{encode, read_all};
use crate::link::DataWanted;

/// What a compile reads besides its files: today's date, the asset paths and the secret
/// schema. Compared whole, so any difference is a miss.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkKey {
    today: Option<Date>,
    assets: Option<Vec<String>>,
    schema: Option<(String, [u8; 32])>,
}

impl LinkKey {
    /// The key of a compile told `today`, `assets` and `schema`.
    pub fn of(today: Option<Date>, assets: Option<&BTreeSet<SourcePath>>, schema: Option<&SourceFile>) -> Self {
        Self {
            today,
            assets: assets.map(|assets| assets.iter().map(|path| path.as_str().to_owned()).collect()),
            schema: schema.map(|schema| (schema.path.as_str().to_owned(), *schema.hash.as_bytes())),
        }
    }
}

/// What a clean compile gives back besides its files and cache.
#[derive(Clone, Debug)]
pub struct Replay {
    /// The program.
    pub program: Arc<Program>,
    /// Every diagnostic, the files' own first, in the order the compile found them.
    pub diagnostics: Diagnostics,
    /// The data files it named that it was not given.
    pub data_wanted: Vec<DataWanted>,
    /// The hook files it named that it was not given.
    pub hooks_wanted: Vec<SourcePath>,
}

/// The output of the compile that `key` describes.
#[derive(Clone, Debug)]
pub struct Memo {
    /// What the compile read besides its files.
    pub key: LinkKey,
    output: Output,
}

#[derive(Clone, Debug)]
enum Output {
    /// Made by a compile in this process.
    Made(Arc<Replay>),
    /// Read back from a blob; decoded only on a hit.
    Bytes(Arc<[u8]>),
}

/// A [`Replay`] as the blob holds it; the program without its files, the diagnostics as JSON.
#[derive(Serialize)]
struct Out<'r> {
    program: &'r Program,
    diagnostics: String,
    data_wanted: Vec<(&'r str, Option<&'r str>)>,
    hooks_wanted: Vec<&'r str>,
}

/// [`Out`], read back.
#[derive(Deserialize)]
struct In {
    program: Program,
    diagnostics: String,
    data_wanted: Vec<(String, Option<String>)>,
    hooks_wanted: Vec<String>,
}

impl Memo {
    /// The memo of a compile in this process.
    pub fn made(key: LinkKey, replay: Replay) -> Self {
        Self { key, output: Output::Made(Arc::new(replay)) }
    }

    /// The memo a blob holds, as `bytes`.
    pub fn read(key: LinkKey, bytes: &[u8]) -> Self {
        Self { key, output: Output::Bytes(Arc::from(bytes)) }
    }

    /// The output again, its program holding `files`; `None` when the bytes do not read back.
    pub fn replay(&self, files: &SourceFiles) -> Option<Replay> {
        match &self.output {
            Output::Made(made) => Some(refiled(made, files)),
            Output::Bytes(bytes) => decode(bytes, files),
        }
    }

    /// The output as bytes, or `None` when it cannot be stored.
    pub fn to_bytes(&self) -> Option<Vec<u8>> {
        let made = match &self.output {
            Output::Made(made) => made,
            Output::Bytes(bytes) => return Some(bytes.to_vec()),
        };
        let out = Out {
            program: &made.program,
            diagnostics: serde_json::to_string(&made.diagnostics).ok()?,
            data_wanted: made
                .data_wanted
                .iter()
                .map(|data| (data.path.as_str(), data.root.as_ref().map(SourcePath::as_str)))
                .collect(),
            hooks_wanted: made.hooks_wanted.iter().map(SourcePath::as_str).collect(),
        };
        encode(&out)
    }
}

/// `made` with a program that holds `files`: the same one when it already does.
fn refiled(made: &Replay, files: &SourceFiles) -> Replay {
    if made.program.files == *files {
        return made.clone();
    }
    let mut program = Program::clone(&made.program);
    program.files = files.clone();
    Replay { program: Arc::new(program), ..made.clone() }
}

fn decode(bytes: &[u8], files: &SourceFiles) -> Option<Replay> {
    let read: In = read_all(bytes)?;
    let mut program = read.program;
    program.files = files.clone();
    let data = read.data_wanted.into_iter().map(|(path, root)| {
        let root = root.map(|root| SourcePath::data(&root)).transpose().ok()?;
        Some(DataWanted { path: SourcePath::data(&path).ok()?, root })
    });
    let hooks = read.hooks_wanted.iter().map(|path| SourcePath::data(path).ok());
    Some(Replay {
        program: Arc::new(program),
        diagnostics: serde_json::from_str(&read.diagnostics).ok()?,
        data_wanted: data.collect::<Option<_>>()?,
        hooks_wanted: hooks.collect::<Option<_>>()?,
    })
}

#[cfg(test)]
#[path = "memo_tests.rs"]
mod tests;
