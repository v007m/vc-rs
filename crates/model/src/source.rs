use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ModelError;

/// A file's path relative to the project root, with `/` separators on every OS.
///
/// Never absolute and never climbs out of the project with `..`. It is what diagnostics and
/// reports print: `services/orders/checkout.test.yaml`, so it holds no control or bidi
/// characters either.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SourcePath(String);

impl SourcePath {
    /// The path `text`, normalised: `\` turned into `/`, and empty (`a//b`) and `.` parts
    /// dropped, so one file has one path.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidSourcePath`] when the path:
    /// - starts with `/`;
    /// - has a `..` part;
    /// - has a `:` in any part. That covers drives (`C:/x`, `C:x`, `C:`), which a join on
    ///   Windows would take out of the project, and NTFS streams (`a.yaml:s`). Windows forbids
    ///   `:` in file names anyway.
    /// - names no file: its last part is empty or `.` (`a/`, `a/.`, `.`, the empty path);
    /// - holds a control character (C0, DEL, C1) or a bidi control (U+202A–U+202E,
    ///   U+2066–U+2069, U+200E, U+200F, U+061C), which could disguise it when printed.
    pub fn new(text: &str) -> Result<Self, ModelError> {
        let slashed = text.replace('\\', "/");
        let names_file = slashed.rsplit('/').next().is_some_and(|last| !last.is_empty() && last != ".");
        let parts: Vec<&str> = slashed.split('/').filter(|part| !part.is_empty() && *part != ".").collect();
        let bad_part = parts.iter().any(|part| *part == ".." || part.contains(':'));
        if slashed.starts_with('/') || !names_file || bad_part || slashed.chars().any(is_hidden) {
            return Err(ModelError::InvalidSourcePath(text.to_owned()));
        }
        Ok(Self(parts.join("/")))
    }

    /// Like [`SourcePath::new`], but a `:`, control or bidi character becomes `�` (U+FFFD)
    /// instead of failing, so a file with a name the project cannot use can still be named in
    /// a diagnostic.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidSourcePath`] when the path is absolute, has a `..` part or names
    /// no file.
    pub fn lossy(text: &str) -> Result<Self, ModelError> {
        let replaced: String = text.chars().map(|c| if c == ':' || is_hidden(c) { '\u{fffd}' } else { c }).collect();
        Self::new(&replaced)
    }

    /// The path of a data file (D173), which may sit outside the project under one of the
    /// config's `datasets.allowRoots`: like [`SourcePath::new`], but a `..` part cancels the
    /// part before it, and the ones left over lead the path (`../shared/cities.csv`).
    ///
    /// # Errors
    ///
    /// What [`SourcePath::new`] rejects, other than `..` parts.
    pub fn data(text: &str) -> Result<Self, ModelError> {
        let slashed = text.replace('\\', "/");
        let mut parts: Vec<&str> = Vec::new();
        for part in slashed.split('/').filter(|part| !part.is_empty() && *part != ".") {
            if part == ".." && parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push(part);
            }
        }
        let climbs = parts.iter().take_while(|part| **part == "..").count();
        let named = slashed.rsplit('/').next().is_some_and(|last| !matches!(last, "" | "." | ".."));
        match Self::new(&parts.get(climbs..).unwrap_or_default().join("/")) {
            Ok(inside) if named && !slashed.starts_with('/') => Ok(Self("../".repeat(climbs) + inside.as_str())),
            _ => Err(ModelError::InvalidSourcePath(text.to_owned())),
        }
    }

    /// Whether the path leads out of the project root (`../shared/cities.csv`).
    #[must_use]
    pub fn is_outside(&self) -> bool {
        self.0.starts_with("../")
    }

    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last part of the path: `checkout.test.yaml`.
    #[must_use]
    pub fn file_name(&self) -> &str {
        crate::basename(&self.0)
    }

    /// Whether the path names a hidden file or folder, or leads through one, as
    /// [`hidden_part`] tells: `.env`, `services/.git/x.api.yaml`, `../shared/cities.csv`.
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        hidden_part(&self.0)
    }
}

/// Whether `text`, one part of a path or a whole one, names a hidden file or folder.
///
/// One of its parts, split at `/` or `\` on every OS, starts with `.`. That covers `.env`,
/// `.git/x`, `..` and `a\.env`, which a system that splits at `\` reads as a hidden file;
/// `a.test.yaml` is not hidden.
#[must_use]
pub fn hidden_part(text: &str) -> bool {
    text.split(['/', '\\']).any(|part| part.starts_with('.'))
}

/// Whether `c` is a control character or a bidi control, which a path must not hold.
const fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{61c}')
}

impl fmt::Display for SourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A file's content hash, opaque here: the engine computes it (BLAKE3) when it reads the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Wraps the 32 hash bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The 32 hash bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The kind of definition file, from its name (syntax reference, "Files at a glance").
///
/// Schemas, datasets and hooks have no kind: they are read because something refers to them
/// (`$file`, `datasets`, `function:`), and that reference says what they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum FileKind {
    /// `vibranium.config.yaml`: project settings, vars, profiles, auth.
    Config,
    /// `*.api.yaml`: how to call endpoints.
    Api,
    /// `*.test.yaml`: tests and their lifecycle blocks.
    Test,
    /// `*.load.yaml`: a load scenario.
    Load,
}

/// Patterns for each kind. A leading `*` stands for a name of at least one character and matches
/// the file name in any folder; a pattern without one is a file at the project root only.
const KIND_PATTERNS: [(&str, FileKind); 4] = [
    ("vibranium.config.yaml", FileKind::Config),
    ("*.api.yaml", FileKind::Api),
    ("*.test.yaml", FileKind::Test),
    ("*.load.yaml", FileKind::Load),
];

impl FileKind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 4] = [Self::Config, Self::Api, Self::Test, Self::Load];

    /// The kind's lowercase name, as in `api.schema.json`: `config`, `api`, `test` or `load`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Api => "api",
            Self::Test => "test",
            Self::Load => "load",
        }
    }

    /// The kind of the file at `path`, or `None` when it matches no kind.
    ///
    /// Only the root `vibranium.config.yaml` is [`FileKind::Config`]; one in a folder has no kind,
    /// and neither has a file outside the project.
    #[must_use]
    pub fn from_path(path: &SourcePath) -> Option<Self> {
        if path.is_outside() {
            return None;
        }
        KIND_PATTERNS.iter().find(|(pattern, _)| path_matches(pattern, path)).map(|&(_, kind)| kind)
    }

    /// The ID of a file of this kind at `path`: the root folder and kind suffix dropped, the rest
    /// joined with dots, and an api file named like its folder collapsed (D139, D142).
    #[must_use]
    pub fn file_id(self, path: &str) -> String {
        let path = ["services/", "load/"].iter().find_map(|root| path.strip_prefix(root)).unwrap_or(path);
        let suffix = format!(".{}.yaml", self.name());
        let path = path.strip_suffix(&suffix).unwrap_or(path);
        let mut words: Vec<&str> = path.split('/').collect();
        let collapses = self == Self::Api && words.len() > 1 && words[words.len() - 1] == words[words.len() - 2];
        if collapses {
            words.pop();
        }
        words.join(".")
    }
}

impl SourcePath {
    /// The ID of the file at this path, as `--changed` matches it; `None` for the config and for
    /// a path of no kind.
    #[must_use]
    pub fn file_id(&self) -> Option<String> {
        let kind = FileKind::from_path(self).filter(|kind| *kind != FileKind::Config)?;
        Some(kind.file_id(self.as_str()))
    }
}

fn path_matches(pattern: &str, path: &SourcePath) -> bool {
    let Some(suffix) = pattern.strip_prefix('*') else { return path.as_str() == pattern };
    let name = path.file_name();
    name.len() > suffix.len() && name.ends_with(suffix)
}

/// One file as the engine read it: what the pure compiler is given instead of doing I/O (D132).
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct SourceFile {
    /// Where the file is, relative to the project root.
    pub path: SourcePath,
    /// The whole file as UTF-8 text.
    pub text: String,
    /// The hash of the file's bytes, used to skip files that did not change.
    pub hash: ContentHash,
}

impl SourceFile {
    /// The file's kind, from its name.
    #[must_use]
    pub fn kind(&self) -> Option<FileKind> {
        FileKind::from_path(&self.path)
    }
}

/// The project path `written` names from `folder` (its parts, outermost first): `.` and empty
/// parts are dropped and `..` climbs; `None` when it climbs out of the project.
#[must_use]
pub fn resolve_path(folder: &[&str], written: &str) -> Option<String> {
    let mut parts = folder.to_vec();
    for part in written.split('/').filter(|part| !part.is_empty() && *part != ".") {
        if part == ".." {
            parts.pop()?;
        } else {
            parts.push(part);
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
