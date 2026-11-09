//! URLs as an importer reads them: the origin a base-URL var holds, and the rest.

/// `scheme://host` and the path after it, for a URL.
pub fn split_origin(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find('/').map_or(url.len(), |at| scheme.len() + 3 + at);
    url.split_at_checked(end)
}
