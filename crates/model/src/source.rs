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

    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last part of the path: `checkout.test.yaml`.
    #[must_use]
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }
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
    /// Only the root `vibranium.config.yaml` is [`FileKind::Config`]; one in a folder has no kind.
    #[must_use]
    pub fn from_path(path: &SourcePath) -> Option<Self> {
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

#[cfg(test)]
mod tests {
    use super::{ContentHash, FileKind, SourcePath};

    fn kind(path: &str) -> Option<FileKind> {
        FileKind::from_path(&SourcePath::new(path).unwrap())
    }

    #[test]
    fn paths_use_forward_slashes_and_drop_leading_dot() {
        let path = SourcePath::new(r".\services\orders\checkout.test.yaml").unwrap();
        assert_eq!(path.as_str(), "services/orders/checkout.test.yaml");
        assert_eq!(path.file_name(), "checkout.test.yaml");
        assert_eq!(path.to_string(), "services/orders/checkout.test.yaml");
        assert_eq!(SourcePath::new("vibranium.config.yaml").unwrap().file_name(), "vibranium.config.yaml");
    }

    #[test]
    fn paths_outside_the_project_are_rejected() {
        for bad in ["", "./", "/etc/passwd", r"C:\x.api.yaml", "../x.api.yaml", "a/../../b", "C:/x", "C:", "c:"] {
            assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
        }
        for bad in ["C:x.api.yaml", "C:..", "c:../x", "a/C:x"] {
            assert!(SourcePath::new(bad).is_err(), "{bad:?} could leave the project on Windows");
        }
    }

    #[test]
    fn paths_collapse_empty_and_dot_parts() {
        for (text, expected) in [("a//b", "a/b"), ("a/./b", "a/b"), ("./a/.//b", "a/b"), (r"a\\b", "a/b")] {
            assert_eq!(SourcePath::new(text).unwrap().as_str(), expected, "{text:?}");
        }
    }

    #[test]
    fn paths_naming_no_file_are_rejected() {
        for bad in [".", "./.", "//", "a/", "a/.", "a/./", "services/orders/", r"a\"] {
            assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn colons_are_rejected_in_every_part() {
        for bad in ["k:v.api.yaml", "services/k:v.api.yaml", "1:/x.api.yaml", "a.yaml:stream"] {
            assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn control_and_bidi_characters_are_rejected() {
        for hidden in ['\x1b', '\t', '\r', '\x7f', '\u{9b}', '\u{202e}', '\u{2066}', '\u{200e}', '\u{200f}', '\u{61c}']
        {
            let bad = format!("services/x{hidden}.test.yaml");
            assert!(SourcePath::new(&bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn lossy_paths_replace_what_new_rejects_but_still_stay_inside() {
        let lossy = SourcePath::lossy("services/k:v\u{202e}.api.yaml").unwrap();
        assert_eq!(lossy.as_str(), "services/k\u{fffd}v\u{fffd}.api.yaml");
        assert_eq!(SourcePath::lossy("a/b.api.yaml").unwrap().as_str(), "a/b.api.yaml");
        for bad in ["/etc/x.api.yaml", "../x.api.yaml", "a/"] {
            assert!(SourcePath::lossy(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn kind_comes_from_the_file_name() {
        assert_eq!(kind("vibranium.config.yaml"), Some(FileKind::Config));
        assert_eq!(kind("services/users/users.api.yaml"), Some(FileKind::Api));
        assert_eq!(kind("services/payments/refunds/partial.test.yaml"), Some(FileKind::Test));
        assert_eq!(kind("load/checkout.load.yaml"), Some(FileKind::Load));
    }

    #[test]
    fn other_names_have_no_kind() {
        for other in ["schemas/user.json", "datasets/cities.csv", "hooks/sign.ts", ".api.yaml", "x.api.yml"] {
            assert_eq!(kind(other), None, "{other}");
        }
        assert_eq!(kind("services/vibranium.config.yaml.bak"), None);
        assert_eq!(kind("services/vibranium.config.yaml"), None, "only the root config is a config");
    }

    #[test]
    fn every_kind_has_its_serde_name() {
        for kind in FileKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.name());
        }
    }

    #[test]
    fn a_path_names_the_file_id_of_its_kind() {
        let id = |path: &str| SourcePath::new(path).unwrap().file_id();
        assert_eq!(id("services/users/users.api.yaml").as_deref(), Some("users"));
        assert_eq!(id("services/users/admin.api.yaml").as_deref(), Some("users.admin"));
        assert_eq!(id("services/users/users.test.yaml").as_deref(), Some("users.users"));
        assert_eq!(id("services/payments/refunds/partial.test.yaml").as_deref(), Some("payments.refunds.partial"));
        assert_eq!(id("load/checkout.load.yaml").as_deref(), Some("checkout"));
        assert_eq!(id("users.api.yaml").as_deref(), Some("users"));
        assert_eq!(id("vibranium.config.yaml"), None);
        assert_eq!(id("datasets/cities.csv"), None);
    }

    #[test]
    fn hash_keeps_its_bytes() {
        assert_eq!(ContentHash::new([7; 32]).as_bytes(), &[7; 32]);
    }
}
