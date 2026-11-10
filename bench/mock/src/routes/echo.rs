use std::collections::BTreeMap;

use axum::body;
use axum::body::Bytes;
use axum::extract::{Query, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Json, Router};
use serde_json::{Value, json};

const BODY_LIMIT: usize = 16 * 1024 * 1024;

/// `ANY /echo` returns the request's method, path, query, headers and body as JSON.
pub fn routes() -> Router {
    Router::new().route("/echo", routing::any(echo))
}

async fn echo(request: Request) -> Response {
    let (parts, body) = request.into_parts();
    match body::to_bytes(body, BODY_LIMIT).await {
        Ok(bytes) => Json(describe(&parts, &bytes)).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

fn describe(parts: &Parts, body: &Bytes) -> Value {
    let query = Query::<BTreeMap<String, String>>::try_from_uri(&parts.uri).map(|query| query.0).unwrap_or_default();
    let headers: BTreeMap<&str, String> = parts
        .headers
        .iter()
        .map(|(name, value)| (name.as_str(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
        .collect();
    json!({
        "method": parts.method.as_str(),
        "path": parts.uri.path(),
        "query": query,
        "headers": headers,
        "body": body_value(body),
    })
}

/// JSON bodies come back parsed, other bodies as text, an empty body as `null`.
fn body_value(body: &Bytes) -> Value {
    if body.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(body).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
}
