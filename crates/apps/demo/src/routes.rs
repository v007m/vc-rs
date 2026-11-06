//! The demo's canned routes: `/users` (list, create, read, remove) and `ANY /echo`. Nothing is
//! stored; a created user gets the next ID and is gone again.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::{self, Body, Bytes};
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use serde_json::{Value, json};

/// The largest body `/echo` reads.
const BODY_LIMIT: usize = 1024 * 1024;

/// The ID the first created user gets, after the two canned ones.
const FIRST_ID: u64 = 3;

/// The demo app.
pub fn router() -> Router {
    let next = Arc::new(AtomicU64::new(FIRST_ID));
    Router::new()
        .route("/users", get(list).post(create))
        .route("/users/{id}", get(read).delete(remove))
        .route("/echo", any(echo))
        .with_state(next)
}

fn users() -> Value {
    json!([{"id": 1, "name": "Ada Lovelace"}, {"id": 2, "name": "Grace Hopper"}])
}

async fn list() -> Json<Value> {
    Json(users())
}

/// `POST /users`: 201 with the next ID and the posted name.
async fn create(State(next): State<Arc<AtomicU64>>, body: Bytes) -> Response {
    let posted: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let name = posted.get("name").cloned().unwrap_or_else(|| json!("someone"));
    let id = next.fetch_add(1, Ordering::Relaxed);
    (StatusCode::CREATED, Json(json!({"id": id, "name": name}))).into_response()
}

/// `GET /users/{id}`: a canned user, or 404.
async fn read(Path(id): Path<u64>) -> Response {
    let found = users().as_array().and_then(|all| all.iter().find(|user| user["id"] == id).cloned());
    found.map_or_else(|| StatusCode::NOT_FOUND.into_response(), |user| Json(user).into_response())
}

async fn remove(Path(_id): Path<u64>) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `ANY /echo`: the request's method, path, query, headers and body as JSON.
async fn echo(request: Request<Body>) -> Response {
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
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(body).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
    };
    json!({"method": parts.method.as_str(), "path": parts.uri.path(), "query": query, "headers": headers, "body": body})
}

#[cfg(test)]
#[path = "routes_tests.rs"]
mod tests;
