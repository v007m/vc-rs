//! Fetching the documents an import reads: the spec itself and the files its `$ref`s name.

use async_trait::async_trait;
use serde_json::Value;

/// Fetches a document an importer needs, by URL or project-relative path.
///
/// The engine implements it over [`crate::Transport`] for URLs (with the api file's
/// `source.headers`, rendered for the active profile) and over [`crate::SourceProvider`] for
/// files, which keeps every read inside the project. Importers resolve relative references
/// with [`resolve_reference`] before they ask.
#[async_trait]
pub trait SpecFetcher: Send + Sync {
    /// The bytes of the document at `location`.
    ///
    /// # Errors
    ///
    /// A [`FetchError`] when the document cannot be read or lies outside what may be read.
    async fn fetch(&self, location: &str) -> Result<Vec<u8>, FetchError>;

    /// The files under the folder at the given location, as paths relative to it with `/`
    /// separators and in path order, for an importer that reads a whole folder (a Bruno
    /// collection); none when the location is a URL, a file, or not there. Hidden files
    /// (`.env`) are never listed. A fetcher that reads no folders lists none.
    ///
    /// # Errors
    ///
    /// A [`FetchError`] when the folder cannot be read or lies outside what may be read.
    async fn list(&self, _folder: &str) -> Result<Vec<String>, FetchError> {
        Ok(Vec::new())
    }

    /// The YAML or JSON document in `bytes`, read from `location`, as a JSON value.
    ///
    /// The engine reads YAML with `vc-parser`, so an importer, which may not depend on it,
    /// decodes through here: the spec it was handed and each document it fetched.
    ///
    /// # Errors
    ///
    /// [`FetchError::Undecodable`] when the bytes are not YAML or JSON.
    fn decode(&self, location: &str, bytes: &[u8]) -> Result<Value, FetchError>;
}

/// Why a document could not be fetched. Messages name the location, never header values.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FetchError {
    /// The request failed, the response was not a success, or the file could not be read.
    #[error("cannot fetch {location}: {reason}")]
    Failed {
        /// The URL or path.
        location: String,
        /// What went wrong.
        reason: String,
    },
    /// The document is not YAML or JSON.
    #[error("{location} is not YAML or JSON: {reason}")]
    Undecodable {
        /// The URL or path.
        location: String,
        /// The first problem the reader found.
        reason: String,
    },
    /// The location lies outside the project, or uses a scheme other than `http` and `https`.
    #[error("{location} may not be fetched: it is outside the project or not http(s)")]
    NotAllowed {
        /// The URL or path.
        location: String,
    },
}

/// The document `reference` names, read relative to the document at `base`, without its `#`
/// fragment: `base` itself for a local `#/...` reference.
///
/// A URL base keeps its scheme and host; a path base stays a path. `None` when the reference
/// climbs above the root of `base` (`../` past its first folder), which would leave the project
/// or the host.
///
/// ```
/// use vc_interfaces::resolve_reference;
///
/// let base = "https://api.example.com/specs/users.yaml";
/// assert_eq!(resolve_reference(base, "common.yaml#/User").as_deref(), Some("https://api.example.com/specs/common.yaml"));
/// assert_eq!(resolve_reference("specs/users.yaml", "../shared/a.yaml").as_deref(), Some("shared/a.yaml"));
/// assert_eq!(resolve_reference("users.yaml", "../a.yaml"), None);
/// ```
#[must_use]
pub fn resolve_reference(base: &str, reference: &str) -> Option<String> {
    let document = reference.split_once('#').map_or(reference, |(document, _)| document);
    if document.is_empty() {
        return Some(base.to_owned());
    }
    if document.contains("://") {
        return Some(document.to_owned());
    }
    let (origin, path) = split_origin(base);
    let rooted = !origin.is_empty() || path.starts_with('/') || document.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    if !document.starts_with('/') {
        segments.extend(path.split('/').filter(|segment| !segment.is_empty()));
        segments.pop();
    }
    for segment in document.split('/').filter(|segment| !segment.is_empty() && *segment != ".") {
        if segment == ".." {
            segments.pop()?;
        } else {
            segments.push(segment);
        }
    }
    let root = if rooted { "/" } else { "" };
    Some(format!("{origin}{root}{}", segments.join("/")))
}

/// `scheme://host` and the path after it for a URL; no origin and the whole text for a path.
fn split_origin(base: &str) -> (&str, &str) {
    let Some((scheme, rest)) = base.split_once("://") else { return ("", base) };
    let host_end = rest.find('/').map_or(base.len(), |at| scheme.len() + 3 + at);
    base.split_at(host_end)
}

#[cfg(test)]
mod tests {
    use super::{FetchError, resolve_reference};

    #[test]
    fn local_references_name_the_base_document() {
        assert_eq!(resolve_reference("specs/a.yaml", "#/components/schemas/User").as_deref(), Some("specs/a.yaml"));
    }

    #[test]
    fn absolute_urls_are_kept() {
        let resolved = resolve_reference("specs/a.yaml", "https://x.test/b.json#/X");
        assert_eq!(resolved.as_deref(), Some("https://x.test/b.json"));
    }

    #[test]
    fn relative_references_join_the_base_folder() {
        assert_eq!(resolve_reference("a.yaml", "b.yaml").as_deref(), Some("b.yaml"));
        assert_eq!(resolve_reference("specs/v1/a.yaml", "./b.yaml").as_deref(), Some("specs/v1/b.yaml"));
        assert_eq!(resolve_reference("specs/v1/a.yaml", "../b.yaml").as_deref(), Some("specs/b.yaml"));
        assert_eq!(resolve_reference("/abs/specs/a.yaml", "b.yaml").as_deref(), Some("/abs/specs/b.yaml"));
    }

    #[test]
    fn url_bases_keep_their_host() {
        let base = "https://x.test/specs/a.yaml";
        assert_eq!(resolve_reference(base, "../b.yaml").as_deref(), Some("https://x.test/b.yaml"));
        assert_eq!(resolve_reference(base, "/other/b.yaml").as_deref(), Some("https://x.test/other/b.yaml"));
        assert_eq!(resolve_reference("https://x.test", "b.yaml").as_deref(), Some("https://x.test/b.yaml"));
    }

    #[test]
    fn climbing_above_the_root_is_refused() {
        assert_eq!(resolve_reference("a.yaml", "../b.yaml"), None);
        assert_eq!(resolve_reference("https://x.test/a.yaml", "../../b.yaml"), None);
    }

    #[test]
    fn errors_name_the_location() {
        let failed = FetchError::Failed { location: "a.yaml".into(), reason: "missing".into() };
        assert_eq!(failed.to_string(), "cannot fetch a.yaml: missing");
        let refused = FetchError::NotAllowed { location: "../a.yaml".into() };
        assert!(refused.to_string().starts_with("../a.yaml may not be fetched"));
        let undecodable = FetchError::Undecodable { location: "a.yaml".into(), reason: "line 2: bad indent".into() };
        assert_eq!(undecodable.to_string(), "a.yaml is not YAML or JSON: line 2: bad indent");
    }
}
