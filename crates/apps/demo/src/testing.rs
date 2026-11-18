//! What the route tests share: a request to the router and its answer.

use axum::Router;
use axum::body::{self, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

/// The status and JSON body (`null` when empty) of `method path` with `body`.
pub async fn send(app: &Router, method: Method, path: &str, body: &str) -> (StatusCode, Value) {
    sent(app, Request::builder().method(method).uri(path), body).await
}

/// The status and JSON body (`null` when empty) of `request` with `body`.
pub async fn sent(app: &Router, request: axum::http::request::Builder, body: &str) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request.body(Body::from(body.to_owned())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
