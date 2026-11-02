use std::path::PathBuf;

/// Failures while loading the workspace model.
#[derive(Debug, thiserror::Error)]
pub enum ArchError {
    /// `cargo metadata` failed or returned output that could not be read.
    #[error("cargo metadata failed: {0}")]
    Metadata(#[from] cargo_metadata::Error),
    /// A source file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A source file is not valid Rust.
    #[error("cannot parse {path}: {source}")]
    Parse {
        /// The file.
        path: String,
        /// The underlying error.
        source: syn::Error,
    },
}
