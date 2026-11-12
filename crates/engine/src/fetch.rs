//! The importers' [`SpecFetcher`]: a URL through the transport, a path through the project's
//! sources (so nothing outside the project is read, nor a hidden file such as `.env` that a
//! pasted spec's `$ref` names), YAML and JSON read with `vc-parser`. A pasted spec's fetcher has
//! no transport, so its `$ref`s never reach the network. A folder (a Bruno collection) is listed
//! through the sources' discovery, so it holds only what discovery reads: no hidden file, nothing
//! the project's ignore files or `node_modules` leave out.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use vc_diagnostics::Diagnostics;
use vc_interfaces::{FetchError, SendContext, SourceFilter, SourceProvider, SpecFetcher, Transport};
use vc_model::{RenderedRequest, SourceFileId, SourcePath, hidden_part};
use vc_parser::Syntax;

use crate::spec_headers::SpecHeaders;

/// Fetches specs for one import or sync.
pub struct ProjectFetcher {
    pub sources: Arc<dyn SourceProvider>,
    /// How URLs are fetched; none for a pasted spec, whose URLs are refused.
    pub transport: Option<Arc<dyn Transport>>,
    /// The `source.headers` sent to the spec's origin, and only there (D313).
    pub headers: SpecHeaders,
}

#[async_trait]
impl SpecFetcher for ProjectFetcher {
    async fn fetch(&self, location: &str) -> Result<Vec<u8>, FetchError> {
        if location.contains("://") { self.get(location).await } else { self.read(location) }
    }

    async fn list(&self, folder: &str) -> Result<Vec<String>, FetchError> {
        let failed = |reason: String| FetchError::Failed { location: folder.to_owned(), reason };
        if folder.contains("://") || hidden_part(folder) {
            return Ok(Vec::new());
        }
        let found = self.sources.discover(&SourceFilter::default()).map_err(|error| failed(error.to_string()))?;
        let prefix = if folder.is_empty() { String::new() } else { format!("{}/", folder.trim_end_matches('/')) };
        let paths = found.files.iter().map(|file| &file.path).chain(&found.assets);
        let mut listed: Vec<String> = paths
            .filter(|path| !path.is_hidden())
            .filter_map(|path| path.as_str().strip_prefix(prefix.as_str()).map(str::to_owned))
            .collect();
        listed.sort();
        Ok(listed)
    }

    fn decode(&self, location: &str, bytes: &[u8]) -> Result<Value, FetchError> {
        let undecodable = |reason: String| FetchError::Undecodable { location: location.to_owned(), reason };
        let text = std::str::from_utf8(bytes).map_err(|error| undecodable(error.to_string()))?;
        let (node, diagnostics) = vc_parser::parse(SourceFileId::new(0), syntax(text), text);
        if let Some(error) = first_error(&diagnostics) {
            return Err(undecodable(error));
        }
        let doc = node.map(|node| vc_compiler::to_json(&node)).filter(|doc| !doc.is_null());
        doc.ok_or_else(|| undecodable("it is empty".to_owned()))
    }
}

impl ProjectFetcher {
    /// This fetcher, sending `headers` to their spec's origin.
    #[must_use]
    pub fn sending(&self, headers: SpecHeaders) -> Self {
        Self { sources: Arc::clone(&self.sources), transport: self.transport.clone(), headers }
    }

    async fn get(&self, location: &str) -> Result<Vec<u8>, FetchError> {
        let failed =
            |reason: String| FetchError::Failed { location: location.to_owned(), reason: self.headers.masked(&reason) };
        let Some(transport) = &self.transport else {
            return Err(failed("a pasted spec may not fetch URLs; import it from its URL with `vc import`".to_owned()));
        };
        let uri = location.parse().map_err(|error: http::uri::InvalidUri| failed(error.to_string()))?;
        let mut request = RenderedRequest::new(http::Method::GET, uri);
        if let Some(headers) = self.headers.for_uri(&request.uri) {
            request.headers.extend(headers.clone());
        }
        let exchange =
            transport.send(request, &SendContext::default()).await.map_err(|error| failed(error.to_string()))?;
        let status = exchange.response.status;
        if !status.is_success() {
            return Err(failed(format!("the server answered {status}")));
        }
        Ok(exchange.response.body.to_vec())
    }

    fn read(&self, location: &str) -> Result<Vec<u8>, FetchError> {
        let not_allowed = || FetchError::NotAllowed { location: location.to_owned() };
        let path = SourcePath::new(location).map_err(|_| not_allowed())?;
        if path.is_hidden() {
            return Err(not_allowed());
        }
        let mut found = self
            .sources
            .read(&[path])
            .map_err(|error| FetchError::Failed { location: location.to_owned(), reason: error.to_string() })?;
        if let Some(problem) = found.problems.first() {
            return Err(FetchError::Failed { location: location.to_owned(), reason: problem.to_string() });
        }
        found.files.pop().map(|file| file.text.as_bytes().to_vec()).ok_or_else(not_allowed)
    }
}

/// JSON when the text starts like JSON, else YAML: a spec is told apart by its content, not
/// its name.
fn syntax(text: &str) -> Syntax {
    if text.trim_start().starts_with(['{', '[']) { Syntax::Json } else { Syntax::Yaml }
}

fn first_error(diagnostics: &Diagnostics) -> Option<String> {
    diagnostics.iter().find(|diagnostic| diagnostic.is_error()).map(|diagnostic| diagnostic.message.clone())
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
