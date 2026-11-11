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
    /// The other files the filter keeps ([`SourceFilter::asset`]) are listed, not read, in
    /// [`Discovery::assets`], so the compiler can check that a `$file:` path exists.
    ///
    /// # Errors
    ///
    /// [`SourceError::Root`] when the project cannot be searched at all.
    fn discover(&self, filter: &SourceFilter) -> Result<Discovery, SourceError>;

    /// The files at `paths`, whatever their kind, read and hashed with [`source_file`], in
    /// path order. The engine reads with it the data and hook files the compiler wants, which
    /// [`Self::discover`] only lists as assets.
    ///
    /// A path that cannot be used (missing, unreadable, not UTF-8, a link out of the project)
    /// goes into [`Discovery::problems`] and the rest are still read; a path given twice is
    /// read once. [`Discovery::assets`] stays empty.
    ///
    /// # Errors
    ///
    /// [`SourceError::Root`] when the project cannot be read at all.
    fn read(&self, paths: &[SourcePath]) -> Result<Discovery, SourceError>;

    /// The bytes of the file at `path`, which need not be text: the engine reads the `$file`
    /// request bodies with it before a run (D272).
    ///
    /// # Errors
    ///
    /// The [`SourceError`] of a path [`Self::read`] cannot use, other than
    /// [`SourceError::NotUtf8`].
    fn read_bytes(&self, path: &SourcePath) -> Result<Vec<u8>, SourceError>;

    /// The data files of `wanted` that lead out of the project, each with the
    /// `datasets.allowRoots` folder it must stay under (D173), read as [`Self::read`] reads, but
    /// only while the file, with links resolved, is inside that folder and in no hidden folder
    /// below it; any other is [`SourceError::OutsideRoot`] in [`Discovery::problems`]. A
    /// provider that holds no files outside the project keeps this default, which reads the
    /// paths with [`Self::read`].
    ///
    /// # Errors
    ///
    /// [`SourceError::Root`] when the project cannot be read at all.
    fn read_outside(&self, wanted: &[(SourcePath, SourcePath)]) -> Result<Discovery, SourceError> {
        let paths: Vec<SourcePath> = wanted.iter().map(|(path, _)| path.clone()).collect();
        self.read(&paths)
    }

    /// Whether the project's ignore files (`.gitignore`, `.ignore`) keep [`Self::discover`] from
    /// the file at `path`, by itself or through a folder it is in; a path that is not there yet
    /// is judged as if it were. The engine asks before it opens or writes a file for the web
    /// UI's editor, together with its [`SourceFilter`]. A provider with no ignore files keeps
    /// this default, which leaves nothing out.
    fn ignores(&self, _path: &SourcePath) -> bool {
        false
    }

    /// Where the file at `path` really is: its path in the project with links resolved, in the
    /// case the file system stores it. `None` when there is no file there, it leads out of the
    /// project, or the provider cannot tell; a provider without links keeps this default.
    fn real(&self, _path: &SourcePath) -> Option<SourcePath> {
        None
    }
}

/// Writes the definition files authoring commands change (D107). The engine calls it only with
/// text its compile guard passed; the implementation knows the project root.
pub trait SourceWriter: Send + Sync {
    /// Replaces the file at `path` with `text`, or creates it with the folders it needs.
    ///
    /// # Errors
    ///
    /// [`SourceError::OutsideRoot`] when `path` leads through a link out of the project, and
    /// [`SourceError::Write`] when the system refuses.
    fn write(&self, path: &SourcePath, text: &str) -> Result<(), SourceError>;

    /// Like [`SourceWriter::write`], but only while the file at `path` is the one a caller read:
    /// its bytes still hash to `was`, or, for `None`, there is no file there. The check is made
    /// just before the file is replaced, which narrows, but does not close, the window in which
    /// another writer's change is lost: no lock is taken.
    ///
    /// # Errors
    ///
    /// [`SourceError::Changed`] when the file is not the one read, and nothing is written; as
    /// [`SourceWriter::write`] otherwise.
    fn write_unchanged(&self, path: &SourcePath, text: &str, was: Option<ContentHash>) -> Result<(), SourceError>;

    /// Removes the file at `path`, which a move (`vc rename` of a file ID) left behind.
    ///
    /// # Errors
    ///
    /// As [`SourceWriter::write`].
    fn remove(&self, path: &SourcePath) -> Result<(), SourceError>;
}

/// What one discovery found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Discovery {
    /// The files read, in path order.
    pub files: Vec<SourceFile>,
    /// The definition files that could not be read, each naming its path.
    pub problems: Vec<SourceError>,
    /// The other files a `$file:` may point at (schemas, specs, bodies), in path order; not read.
    pub assets: Vec<SourcePath>,
}

impl Discovery {
    /// Puts the files, problems and assets in path order, as [`SourceProvider::discover`]
    /// returns them.
    pub fn sort(&mut self) {
        self.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.assets.sort();
        self.problems.sort_by(|a, b| a.path().cmp(&b.path()));
    }
}

/// Why a file, or the whole search, could not be read.
///
/// Paths are relative to the project root with `/` separators. They are plain text, not
/// [`SourcePath`]s, because some of them are exactly the names a `SourcePath` refuses.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SourceError {
    /// No file is there.
    #[error("{path}: no such file")]
    Missing {
        /// The path.
        path: String,
    },
    /// The file exists but reading it failed.
    #[error("{path}: cannot be read: {reason}")]
    Read {
        /// The file.
        path: String,
        /// What the system said.
        reason: String,
    },
    /// Writing the file failed.
    #[error("{path}: cannot be written: {reason}")]
    Write {
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
    /// The file is a link to something outside the project, or into a hidden folder or file
    /// such as `.env`, which is never read or written.
    #[error("{path}: links to a file outside the project or a hidden one")]
    OutsideRoot {
        /// The link.
        path: String,
    },
    /// The file changed, appeared or was removed since it was read, so it was not written.
    #[error("{path}: changed since it was read")]
    Changed {
        /// The file.
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
            Self::Missing { path }
            | Self::Read { path, .. }
            | Self::Write { path, .. }
            | Self::NotUtf8 { path }
            | Self::InvalidPath { path }
            | Self::OutsideRoot { path }
            | Self::Changed { path } => Some(path),
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
