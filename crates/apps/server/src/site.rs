//! The web UI's files, served for any `GET` the API and the mounts do not answer. A path that is
//! not a file gets `index.html`, so the single-page app routes it in the browser; one that looks
//! like a file (it has an extension) and is missing gets 404.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

/// The page the app starts from, and the one unknown paths get.
pub const INDEX: &str = "index.html";

/// The built web UI, looked up by path under its root (`index.html`, `_app/immutable/app.js`).
/// The lookup is by name only, so a path can never reach outside the files.
pub trait Files: Send + Sync + 'static {
    /// The file's bytes, if there is such a file.
    fn get(&self, path: &str) -> Option<Bytes>;
}

/// The media type of each file extension the web UI is built with.
const MEDIA_TYPES: &[(&str, &str)] = &[
    ("html", "text/html; charset=utf-8"),
    ("js", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json"),
    ("webmanifest", "application/manifest+json"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("ico", "image/x-icon"),
    ("woff2", "font/woff2"),
    ("txt", "text/plain; charset=utf-8"),
];

/// The fallback route: a file, the app's `index.html`, or 404.
pub async fn serve(State(files): State<Option<Arc<dyn Files>>>, request: Request) -> Response {
    let (parts, _) = request.into_parts();
    let path = parts.uri.path().trim_start_matches('/');
    let readable = parts.method == Method::GET || parts.method == Method::HEAD;
    let found = files.filter(|_| readable && !path.starts_with("api/")).and_then(|files| page(files.as_ref(), path));
    match found {
        Some((name, bytes)) => ([(header::CONTENT_TYPE, media_type(name))], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn page<'a>(files: &dyn Files, path: &'a str) -> Option<(&'a str, Bytes)> {
    if let Some(bytes) = files.get(path) {
        return Some((path, bytes));
    }
    let file_like = path.rsplit('/').next().is_some_and(|name| name.contains('.'));
    if file_like { None } else { files.get(INDEX).map(|bytes| (INDEX, bytes)) }
}

fn media_type(path: &str) -> &'static str {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    let known = extension.and_then(|ext| MEDIA_TYPES.iter().find(|(name, _)| ext.eq_ignore_ascii_case(name)));
    known.map_or("application/octet-stream", |(_, media_type)| media_type)
}
