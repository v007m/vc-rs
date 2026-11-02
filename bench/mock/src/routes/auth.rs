use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::error::MockError;
use crate::store::Store;

const EXPIRES_IN_SECS: u64 = 3600;

/// `POST /auth/token` issues a bearer token; `GET /auth/me` accepts only issued tokens.
pub fn routes(store: &Store) -> Router {
    Router::new().route("/auth/token", post(token)).route("/auth/me", get(me)).with_state(store.clone())
}

async fn token(State(store): State<Store>) -> Result<Json<Value>, MockError> {
    let token = store.call(|state| state.sessions.login()).await?;
    Ok(Json(json!({ "access_token": token, "token_type": "Bearer", "expires_in": EXPIRES_IN_SECS })))
}

async fn me(State(store): State<Store>, headers: HeaderMap) -> Result<Response, MockError> {
    let Some(token) = bearer(&headers) else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let checked = token.clone();
    if store.call(move |state| state.sessions.is_valid(&checked)).await? {
        Ok(Json(json!({ "user": "mock", "token": token })).into_response())
    } else {
        Ok(StatusCode::UNAUTHORIZED.into_response())
    }
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(str::to_owned)
}
