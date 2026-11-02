use std::fmt;

use serde::Serialize;

use crate::ModelError;

/// A file's path relative to the project root, with `/` separators on every OS.
///
/// Never absolute and never climbs out of the project with `..`. It is what diagnostics and
/// reports print: `services/orders/checkout.test.yaml`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct SourcePath(String);

impl SourcePath {
    /// The path `text`, with `\` turned into `/` and any leading `./` dropped.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidSourcePath`] when the path is empty, absolute (`/x`, `C:/x`) or has
    /// a `..` part.
    pub fn new(text: &str) -> Result<Self, ModelError> {
        let slashed = text.replace('\\', "/");
        let path = slashed.trim_start_matches("./");
        let absolute = path.starts_with('/') || path.as_bytes().get(1) == Some(&b':');
        if path.is_empty() || absolute || path.split('/').any(|part| part == "..") {
            return Err(ModelError::InvalidSourcePath(text.to_owned()));
        }
        Ok(Self(path.to_owned()))
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
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

/// File-name patterns for each kind; a leading `*` stands for a name of at least one character.
const KIND_PATTERNS: [(&str, FileKind); 4] = [
    ("vibranium.config.yaml", FileKind::Config),
    ("*.api.yaml", FileKind::Api),
    ("*.test.yaml", FileKind::Test),
    ("*.load.yaml", FileKind::Load),
];

impl FileKind {
    /// The kind of the file at `path`, or `None` when its name matches no kind.
    #[must_use]
    pub fn from_path(path: &SourcePath) -> Option<Self> {
        let name = path.file_name();
        KIND_PATTERNS.iter().find(|(pattern, _)| name_matches(pattern, name)).map(|&(_, kind)| kind)
    }
}

fn name_matches(pattern: &str, name: &str) -> bool {
    pattern.strip_prefix('*').map_or(name == pattern, |suffix| name.len() > suffix.len() && name.ends_with(suffix))
}

/// One file as the engine read it: what the pure compiler is given instead of doing I/O (D132).
#[derive(Clone, Debug, PartialEq, Eq)]
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
        for bad in ["", "./", "/etc/passwd", r"C:\x.api.yaml", "../x.api.yaml", "a/../../b"] {
            assert!(SourcePath::new(bad).is_err(), "{bad:?} was accepted");
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
    }

    #[test]
    fn hash_keeps_its_bytes() {
        assert_eq!(ContentHash::new([7; 32]).as_bytes(), &[7; 32]);
    }
}
