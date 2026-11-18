//! The users API's `OpenAPI` specs (D331), served as files: `users.yaml` describes `/users` as the
//! demo serves it, and `users-v1.yaml` and `users-v2.yaml` are two older versions, for trying
//! `vc sync` on a spec that changes. Each names the server it was asked on.

use axum::Router;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::scripted::Segment;

/// The specs, by file name.
pub const SPECS: [(&str, &str); 3] = [
    ("users.yaml", include_str!("../specs/users.yaml")),
    ("users-v1.yaml", include_str!("../specs/users-v1.yaml")),
    ("users-v2.yaml", include_str!("../specs/users-v2.yaml")),
];

/// The server each spec names, replaced by the host it was asked on.
const SERVER: &str = "http://localhost:7421";

/// The longest host a spec names; a longer one keeps [`SERVER`].
const MAX_HOST_BYTES: usize = 255;

pub fn routes() -> Router {
    Router::new().route("/openapi/{name}", get(spec))
}

/// `GET /openapi/{name}`: the spec, as YAML, naming the server at the request's `Host`; else 404.
async fn spec(name: Segment, headers: HeaderMap) -> Response {
    let found = name.ok().and_then(|Path(name)| SPECS.iter().find(|(file, _)| *file == name));
    let Some((_, text)) = found else { return StatusCode::NOT_FOUND.into_response() };
    let host = headers.get(header::HOST).and_then(|host| host.to_str().ok()).filter(|host| plain(host));
    let text = host.map_or_else(|| (*text).to_owned(), |host| text.replace(SERVER, &format!("http://{host}")));
    ([(header::CONTENT_TYPE, "application/yaml")], text).into_response()
}

/// Whether `host` is a plain host and port (letters, digits, `.`, `-`, `:` and the brackets of
/// an IPv6 address), so it cannot add YAML to the spec.
fn plain(host: &str) -> bool {
    let allowed = |byte: u8| byte.is_ascii_alphanumeric() || b".-:[]".contains(&byte);
    !host.is_empty() && host.len() <= MAX_HOST_BYTES && host.bytes().all(allowed)
}

#[cfg(test)]
#[path = "openapi_tests.rs"]
mod tests;
