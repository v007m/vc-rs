use axum::extract::State;
use axum::http::StatusCode;
use axum::routing;
use axum::{Json, Router};

use crate::error::MockError;
use crate::state::State as MockState;
use crate::stats::Stats;
use crate::store::Store;

/// `GET /__stats` and `POST /__reset`. These are not counted themselves.
pub fn routes(store: &Store) -> Router {
    Router::new()
        .route("/__stats", routing::get(stats))
        .route("/__reset", routing::post(reset))
        .with_state(store.clone())
}

async fn stats(State(store): State<Store>) -> Result<Json<Stats>, MockError> {
    Ok(Json(store.call(|state| state.stats()).await?))
}

async fn reset(State(store): State<Store>) -> Result<StatusCode, MockError> {
    store.call(MockState::reset).await?;
    Ok(StatusCode::NO_CONTENT)
}
