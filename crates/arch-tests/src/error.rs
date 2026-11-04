use std::path::PathBuf;

/// Failures while loading the workspace model.
#[derive(Debug, thiserror::Error)]
#[expect(clippy::exhaustive_enums, reason = "internal gate crate; no external users")]
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
    /// A `mod name;` item has no `name.rs` or `name/mod.rs` file.
    #[error("module `{name}` has no file in {dir}")]
    Module {
        /// The directory the module was looked for in.
        dir: PathBuf,
        /// The module name.
        name: String,
    },
}
