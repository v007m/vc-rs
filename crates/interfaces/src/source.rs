use vc_model::{ContentHash, SourceFile, SourcePath};

use crate::SourceFilter;

/// Finds and reads the project's definition files (D132).
///
/// The implementation knows the project root; the engine only says which files it wants.
pub trait SourceProvider: Send + Sync {
    /// Every file under the root whose name has a [`FileKind`](vc_model::FileKind) and that
    /// `filter` allows, read and hashed with [`source_file`], in path order.
    ///
    /// A file that cannot be used (unreadable, not UTF-8, a name a [`SourcePath`] refuses, a
    /// link out of the project) goes into [`Discovery::problems`] and the rest are still read.
    ///
    /// # Errors
    ///
    /// [`SourceError::Root`] when the project cannot be searched at all.
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError>;
}

/// What one discovery found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    /// The files read, in path order.
    pub files: Vec<SourceFile>,
    /// The definition files that could not be read, each naming its path.
    pub problems: Vec<SourceError>,
}

impl Discovery {
    /// Puts the files and the problems in path order, as [`SourceProvider::discover`] returns them.
    pub fn sort(&mut self) {
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.problems.sort_by(|a, b| a.path().cmp(&b.path()));
    }
}

/// Why a file, or the whole search, could not be read.
///
/// Paths are relative to the project root with `/` separators. They are plain text, not
/// [`SourcePath`]s, because some of them are exactly the names a `SourcePath` refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    /// The file exists but reading it failed.
    #[error("{path}: cannot be read: {reason}")]
    Read {
        /// The file.
        path: String,
        /// What the system said.
        reason: String,
    },
    /// The file is not UTF-8 text.
    #[error("{path}: is not UTF-8 text")]
    NotUtf8 {
        /// The file.
        path: String,
    },
    /// The name holds a `:`, a control or bidi character, or is not UTF-8.
    #[error("{path}: the file name cannot be used in a project")]
    InvalidPath {
        /// The name, with what could not be decoded replaced.
        path: String,
    },
    /// The file is a link to something outside the project, which is never read.
    #[error("{path}: links to a file outside the project")]
    OutsideRoot {
        /// The link.
        path: String,
    },
    /// A `paths.include` or `paths.exclude` glob does not parse.
    #[error("invalid glob {glob:?}: {reason}")]
    InvalidGlob {
        /// The glob as written.
        glob: String,
        /// What is wrong with it.
        reason: String,
    },
    /// The project root itself cannot be searched.
    #[error("cannot search the project at {root}: {reason}")]
    Root {
        /// The root.
        root: String,
        /// What the system said.
        reason: String,
    },
}

impl SourceError {
    /// The file this problem is about, or `None` for a problem with the whole search.
    #[must_use]
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Read { path, .. }
            | Self::NotUtf8 { path }
            | Self::InvalidPath { path }
            | Self::OutsideRoot { path } => Some(path),
            Self::InvalidGlob { .. } | Self::Root { .. } => None,
        }
    }
}

/// The BLAKE3 hash of a file's bytes, which the compile cache compares between runs.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> ContentHash {
    ContentHash::new(*blake3::hash(bytes).as_bytes())
}

/// The file at `path` with content `bytes`, hashed before it is decoded.
///
/// # Errors
///
/// [`SourceError::NotUtf8`] when the bytes are not UTF-8.
pub fn source_file(path: SourcePath, bytes: Vec<u8>) -> Result<SourceFile, SourceError> {
    let hash = content_hash(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| SourceError::NotUtf8 { path: path.to_string() })?;
    Ok(SourceFile { path, text, hash })
}

#[cfg(test)]
mod tests {
    use vc_model::SourcePath;

    use super::{SourceError, content_hash, source_file};

    #[test]
    fn hashes_are_blake3_of_the_bytes() {
        let empty = content_hash(b"");
        assert_eq!(empty.as_bytes()[..4], [0xaf, 0x13, 0x49, 0xb9]);
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
    }

    #[test]
    fn files_must_be_utf8() {
        let path = SourcePath::new("a.api.yaml").unwrap();
        let file = source_file(path.clone(), b"x: 1\n".to_vec()).unwrap();
        assert_eq!((file.text.as_str(), file.hash), ("x: 1\n", content_hash(b"x: 1\n")));
        let error = source_file(path, vec![0xff]).unwrap_err();
        assert_eq!(error, SourceError::NotUtf8 { path: "a.api.yaml".to_owned() });
        assert_eq!(error.path(), Some("a.api.yaml"));
    }

    #[test]
    fn whole_search_problems_name_no_file() {
        let root = SourceError::Root { root: "/p".to_owned(), reason: "gone".to_owned() };
        assert_eq!(root.path(), None);
        assert_eq!(root.to_string(), "cannot search the project at /p: gone");
    }
}
