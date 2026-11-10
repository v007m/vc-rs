//! The bytes of the `$file` request bodies, read by the engine before the run so the executor
//! does no I/O (D272).

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;

/// A file's bytes, or why it could not be read.
type Read = Result<Bytes, String>;

/// Each `$file` body's bytes by its project path, or why the file could not be read.
#[derive(Clone, Debug, Default)]
pub struct BodyFiles(Arc<HashMap<String, Read>>);

impl BodyFiles {
    /// The files read, each by its project path.
    #[must_use]
    pub fn new(files: impl IntoIterator<Item = (String, Result<Vec<u8>, String>)>) -> Self {
        Self(Arc::new(files.into_iter().map(|(path, read)| (path, read.map(Bytes::from))).collect()))
    }

    /// The bytes of the file at `path`, or why it could not be read; `None` when the run was
    /// not given it, as for a templated path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<Result<Bytes, &str>> {
        self.0.get(path).map(|read| match read {
            Ok(bytes) => Ok(bytes.clone()),
            Err(reason) => Err(reason.as_str()),
        })
    }
}
