//! `vc import curl`: one api per curl command, as browser dev tools and API docs copy them.

mod body;
mod draft;
mod options;
mod shell;

use async_trait::async_trait;
use vc_interfaces::{ImportDraft, ImportError, ImportOptions, ImportSource, Importer};

/// Reads a file of curl commands into a draft api file.
///
/// It drafts one api per distinct method and path, all sent to one origin. A credential (`authorization`, a cookie, `-u`, a URL's password) is
/// never written; the notes say where one was left out.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct CurlImporter;

#[async_trait]
impl Importer for CurlImporter {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError> {
        let invalid = |reason: String| ImportError::Invalid { location: source.location.clone(), reason };
        let text = std::str::from_utf8(&source.bytes).map_err(|_| invalid("it is not UTF-8 text".to_owned()))?;
        let words = shell::words(text).map_err(invalid)?;
        let requests: Vec<_> = shell::commands(&words).into_iter().map(options::request).collect();
        if requests.is_empty() {
            return Err(invalid("it holds no curl command".to_owned()));
        }
        draft::draft(&requests, opts).map_err(invalid)
    }
}

#[cfg(test)]
mod tests;
