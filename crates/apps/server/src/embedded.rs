//! The web UI built into the binary: `web/build`, which `pnpm -C web build` writes. Debug builds
//! embed the files too (`debug-embed`), so nothing is read from disk at run time and there is no
//! global path cache. A build made before `web/build` exists has no files, and
//! [`EmbeddedUi::is_empty`] says so.

use std::borrow::Cow;

use bytes::Bytes;
use rust_embed::RustEmbed;

use crate::site::{Files, INDEX};

#[derive(RustEmbed)]
#[folder = "../../../web/build"]
#[allow_missing = true]
struct Build;

/// The web UI files built into this binary.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct EmbeddedUi;

impl EmbeddedUi {
    /// Whether the binary was built without the web UI's `index.html`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        Build::get(INDEX).is_none()
    }
}

impl Files for EmbeddedUi {
    fn get(&self, path: &str) -> Option<Bytes> {
        Build::get(path).map(|file| match file.data {
            Cow::Borrowed(data) => Bytes::from_static(data),
            Cow::Owned(data) => Bytes::from(data),
        })
    }
}
