/// Keeps the compiler's cache (`CompileCache::to_bytes`) between runs as one opaque blob (D132).
///
/// The system adapter keeps it at `.vibranium/cache/compile.bin`. The blob may be stale or
/// damaged; the compiler then reads it as an empty cache, so a store never checks it.
pub trait CompileCacheStore: Send + Sync {
    /// The bytes saved last, or `None` when nothing was saved yet.
    ///
    /// # Errors
    ///
    /// [`CompileCacheError::Read`] when the blob exists but cannot be read.
    fn load(&self) -> Result<Option<Vec<u8>>, CompileCacheError>;

    /// Replaces the saved bytes. A reader sees the old blob or the new one, never a mix.
    ///
    /// # Errors
    ///
    /// [`CompileCacheError::Write`] when the blob cannot be written.
    fn save(&self, bytes: &[u8]) -> Result<(), CompileCacheError>;
}

/// Why the compile cache could not be read or written. Neither stops a check: without a cache
/// the compiler only does more work.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CompileCacheError {
    /// Reading the blob failed.
    #[error("cannot read the compile cache at {location}: {reason}")]
    Read {
        /// Where the blob is.
        location: String,
        /// What went wrong.
        reason: String,
    },
    /// Writing the blob failed.
    #[error("cannot write the compile cache at {location}: {reason}")]
    Write {
        /// Where the blob is.
        location: String,
        /// What went wrong.
        reason: String,
    },
}
