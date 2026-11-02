use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Map, Value};

use crate::error::MockError;
use crate::store::Store;

type Item = Map<String, Value>;

#[derive(Clone)]
struct Crud {
    store: Store,
    kind: &'static str,
}

/// `/{kind}` (list, create) and `/{kind}/{id}` (get, put, patch, delete).
pub fn routes(store: &Store, kind: &'static str) -> Router {
    let crud = Crud { store: store.clone(), kind };
    Router::new()
        .route(&format!("/{kind}"), get(list).post(create))
        .route(&format!("/{kind}/{{id}}"), get(read).put(replace).patch(merge).delete(remove))
        .with_state(crud)
}

async fn list(State(crud): State<Crud>) -> Result<Json<Value>, MockError> {
    let kind = crud.kind;
    Ok(Json(crud.store.call(move |state| state.collections.list(kind)).await?))
}

async fn create(State(crud): State<Crud>, Json(item): Json<Item>) -> Result<(StatusCode, Json<Value>), MockError> {
    let kind = crud.kind;
    let created = crud.store.call(move |state| state.collections.create(kind, item)).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn read(State(crud): State<Crud>, Path(id): Path<u64>) -> Result<Response, MockError> {
    let kind = crud.kind;
    Ok(found(crud.store.call(move |state| state.collections.get(kind, id)).await?))
}

async fn replace(State(crud): State<Crud>, Path(id): Path<u64>, Json(item): Json<Item>) -> Result<Response, MockError> {
    let kind = crud.kind;
    Ok(found(crud.store.call(move |state| state.collections.replace(kind, id, item)).await?))
}

async fn merge(State(crud): State<Crud>, Path(id): Path<u64>, Json(patch): Json<Item>) -> Result<Response, MockError> {
    let kind = crud.kind;
    Ok(found(crud.store.call(move |state| state.collections.merge(kind, id, patch)).await?))
}

async fn remove(State(crud): State<Crud>, Path(id): Path<u64>) -> Result<StatusCode, MockError> {
    let kind = crud.kind;
    let deleted = crud.store.call(move |state| state.collections.delete(kind, id)).await?;
    Ok(if deleted { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
}

fn found(item: Option<Value>) -> Response {
    item.map_or_else(|| StatusCode::NOT_FOUND.into_response(), |item| Json(item).into_response())
}
