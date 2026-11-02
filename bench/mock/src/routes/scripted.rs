use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::MockError;
use crate::store::Store;

const KB_CHUNK: usize = 1000;

/// Routes with scripted behaviour: delays, failures, polling, fixed statuses and large bodies.
pub fn routes(store: &Store) -> Router {
    Router::new()
        .route("/slow", get(slow))
        .route("/flaky", get(flaky))
        .route("/poll/{id}", get(poll))
        .route("/status/{code}", get(status))
        .route("/large", get(large))
        .with_state(store.clone())
}

#[derive(Deserialize)]
struct SlowQuery {
    #[serde(default)]
    ms: u64,
}

/// `GET /slow?ms=n` answers after `n` milliseconds.
async fn slow(Query(query): Query<SlowQuery>) -> Json<Value> {
    tokio::time::sleep(Duration::from_millis(query.ms)).await;
    Json(json!({ "sleptMs": query.ms }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlakyQuery {
    #[serde(default)]
    key: String,
    #[serde(default)]
    fail_times: u64,
}

/// `GET /flaky?key=k&failTimes=n`: the first `n` calls per key get 503, later calls 200.
async fn flaky(State(store): State<Store>, Query(query): Query<FlakyQuery>) -> Result<Response, MockError> {
    let key = query.key;
    let attempt = store.call(move |state| state.flaky.bump(key)).await?;
    let status = if attempt <= query.fail_times { StatusCode::SERVICE_UNAVAILABLE } else { StatusCode::OK };
    Ok((status, Json(json!({ "attempt": attempt }))).into_response())
}

#[derive(Deserialize)]
struct PollQuery {
    #[serde(default)]
    after: u64,
}

/// `GET /poll/{id}?after=n`: `pending` until the `n`th call for that id, which and later calls get `done`.
async fn poll(
    State(store): State<Store>,
    Path(id): Path<String>,
    Query(query): Query<PollQuery>,
) -> Result<Json<Value>, MockError> {
    let attempt = store.call(move |state| state.polls.bump(id)).await?;
    let status = if attempt >= query.after { "done" } else { "pending" };
    Ok(Json(json!({ "status": status, "attempt": attempt })))
}

/// `GET /status/{code}` answers with that status; an invalid code gets 400.
async fn status(Path(code): Path<u16>) -> Response {
    StatusCode::from_u16(code).map_or_else(
        |_| StatusCode::BAD_REQUEST.into_response(),
        |status| (status, Json(json!({ "status": code }))).into_response(),
    )
}

#[derive(Deserialize)]
struct LargeQuery {
    #[serde(default)]
    kb: usize,
}

/// `GET /large?kb=n` returns a JSON array of `n` strings of 1000 bytes each (about `n` KB).
async fn large(Query(query): Query<LargeQuery>) -> Json<Value> {
    let chunk = Value::String("x".repeat(KB_CHUNK));
    Json(json!({ "items": vec![chunk; query.kb] }))
}
