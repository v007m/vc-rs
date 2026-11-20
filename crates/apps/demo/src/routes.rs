//! The demo's router: `/users`, kept in [`crate::users`], and `ANY /echo` here; the test routes
//! in [`crate::scripted`], server-sent events in [`crate::events`], sign-in in [`crate::auth`] and the users specs in [`crate::openapi`].

use std::collections::BTreeMap;

use axum::body::{self, Body, Bytes};
use axum::extract::{Query, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::{auth, events, openapi, scripted, users};

/// The largest body `/echo` reads.
const BODY_LIMIT: usize = 1024 * 1024;

/// The header `/echo` names this process in, so `vc demo start` knows the demo it launched is the
/// one that answers.
pub const PID_HEADER: &str = "x-vc-demo-pid";

/// The demo app.
pub fn router() -> Router {
    Router::new()
        .route("/echo", any(echo))
        .merge(users::routes())
        .merge(scripted::routes())
        .merge(events::routes())
        .merge(auth::routes())
        .merge(openapi::routes())
}

/// `ANY /echo`: the request's method, path, query, headers and body as JSON, with this process's
/// pid in [`PID_HEADER`].
async fn echo(request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = match body::to_bytes(body, BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    let echoed = json!({
        "method": parts.method.as_str(),
        "path": parts.uri.path(),
        "query": query_of(&parts),
        "headers": headers_of(&parts),
        "body": body_of(&bytes),
    });
    let mut response = Json(echoed).into_response();
    // A WASI build (the docs playground, which serves the demo in the page) has no processes, and
    // `std::process::id` panics there, so it names none, as an older demo did.
    if cfg!(not(target_os = "wasi")) {
        response.headers_mut().insert(PID_HEADER, std::process::id().into());
    }
    response
}

fn query_of(parts: &Parts) -> BTreeMap<String, String> {
    Query::try_from_uri(&parts.uri).map(|query| query.0).unwrap_or_default()
}

fn headers_of(parts: &Parts) -> BTreeMap<&str, String> {
    let text = |value: &[u8]| String::from_utf8_lossy(value).into_owned();
    parts.headers.iter().map(|(name, value)| (name.as_str(), text(value.as_bytes()))).collect()
}

/// The body as JSON, as text when it is not JSON, or null when there is none.
fn body_of(bytes: &Bytes) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;
