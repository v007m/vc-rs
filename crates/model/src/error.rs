/// Errors raised while building model data.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    /// The string table already holds as many strings as a [`crate::Sym`] can address.
    #[error("the string table is full ({0} strings)")]
    StringTableFull(usize),
    /// A file path that is empty, absolute or climbs out of the project with `..`.
    #[error("`{0}` is not a path inside the project")]
    InvalidSourcePath(String),
    /// The file table already holds as many files as a [`crate::SourceFileId`] can address.
    #[error("the file table is full ({0} files)")]
    FileTableFull(usize),
}
