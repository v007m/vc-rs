use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Json, Router};
use serde_json::{Map, Value};

use crate::error::MockError;
use crate::store::Store;

type Item = Map<String, Value>;

/// The collection a user's orders are kept in, apart from `/orders`.
const ORDERS: &str = "user orders";

/// `/users/{id}/orders` (list, create) and `/users/{id}/orders/{orderId}` (get, delete): orders
/// kept per user, each holding its user's id as `userId`. Under a user that does not exist, or
/// another user's order, the answer is not found.
pub fn routes(store: &Store) -> Router {
    Router::new()
        .route("/users/{id}/orders", routing::get(list).post(create))
        .route("/users/{id}/orders/{orderId}", routing::get(read).delete(remove))
        .with_state(store.clone())
}

async fn list(State(store): State<Store>, Path(user): Path<u64>) -> Result<Json<Value>, MockError> {
    let all = store.call(|state| state.collections.list(ORDERS)).await?;
    let owned = all.as_array().into_iter().flatten().filter(|order| is_of(order, user)).cloned().collect();
    Ok(Json(Value::Array(owned)))
}

async fn create(
    State(store): State<Store>,
    Path(user): Path<u64>,
    Json(mut order): Json<Item>,
) -> Result<Response, MockError> {
    order.insert("userId".to_owned(), user.into());
    let created = store
        .call(move |state| state.collections.get("users", user).map(|_| state.collections.create(ORDERS, order)))
        .await?;
    Ok(created.map_or_else(
        || StatusCode::NOT_FOUND.into_response(),
        |order| (StatusCode::CREATED, Json(order)).into_response(),
    ))
}

async fn read(State(store): State<Store>, Path((user, id)): Path<(u64, u64)>) -> Result<Response, MockError> {
    let order = store.call(move |state| state.collections.get(ORDERS, id)).await?;
    let found = order.filter(|order| is_of(order, user));
    Ok(found.map_or_else(|| StatusCode::NOT_FOUND.into_response(), |order| Json(order).into_response()))
}

async fn remove(State(store): State<Store>, Path((user, id)): Path<(u64, u64)>) -> Result<StatusCode, MockError> {
    let deleted = store
        .call(move |state| {
            let owned = state.collections.get(ORDERS, id).is_some_and(|order| is_of(&order, user));
            owned && state.collections.delete(ORDERS, id)
        })
        .await?;
    Ok(if deleted { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

/// Whether `order` belongs to the user `user`.
fn is_of(order: &Value, user: u64) -> bool {
    order.get("userId").and_then(Value::as_u64) == Some(user)
}
