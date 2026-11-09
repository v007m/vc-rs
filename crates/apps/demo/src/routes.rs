//! The demo's canned routes: `/users` (list, create, read, remove) and `ANY /echo`. Nothing is
//! stored; a created user gets the next ID, can be read back by it, and needs an email.

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
    json!([{"id": "1", "name": "Ada Lovelace"}, {"id": "2", "name": "Grace Hopper"}])
}

async fn list() -> Json<Value> {
    Json(users())
}

/// `POST /users`: 201 with the next ID, as text, and the posted name and email. Without an email,
/// 400 with a null ID: the sample's `createUser` saves `response.body.id` whenever its checks hold.
async fn create(State(next): State<Arc<AtomicU64>>, body: Bytes) -> Response {
    let posted: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let Some(email) = posted.get("email").filter(|email| email.is_string()).cloned() else {
        return (StatusCode::BAD_REQUEST, Json(json!({"id": null, "error": "a user needs an email"}))).into_response();
    };
    let name = posted.get("name").cloned().unwrap_or_else(|| json!("someone"));
    let id = next.fetch_add(1, Ordering::Relaxed).to_string();
    (StatusCode::CREATED, Json(json!({"id": id, "name": name, "email": email}))).into_response()
}

/// `GET /users/{id}`: a canned user, a created one by its ID alone (nothing is stored), or 404.
async fn read(State(next): State<Arc<AtomicU64>>, Path(id): Path<String>) -> Response {
    let canned = users().as_array().and_then(|all| all.iter().find(|user| user["id"] == id.as_str()).cloned());
    let created = id.parse::<u64>().is_ok_and(|n| (FIRST_ID..next.load(Ordering::Relaxed)).contains(&n));
    match canned {
        Some(user) => Json(user).into_response(),
        None if created => Json(json!({"id": id})).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn remove(Path(_id): Path<String>) -> StatusCode {
    StatusCode::NO_CONTENT
}

/// `ANY /echo`: the request's method, path, query, headers and body as JSON.
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
    Json(echoed).into_response()
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
