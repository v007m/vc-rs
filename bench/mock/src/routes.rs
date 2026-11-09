mod admin;
mod auth;
mod crud;
mod echo;
mod oauth;
mod openapi;
mod scripted;
mod track;

use axum::Router;
use axum::middleware;
use tower_http::compression::CompressionLayer;

use crate::store::Store;

/// Builds the app. Every route except `/__stats`, `/__reset` and `/__openapi/…` is counted in
/// [`crate::Stats`].
/// Responses are gzip- or brotli-compressed when the client's `Accept-Encoding` allows it.
pub fn router(store: &Store) -> Router {
    let counted = Router::new()
        .merge(crud::routes(store, "users"))
        .merge(crud::routes(store, "orders"))
        .merge(auth::routes(store))
        .merge(echo::routes())
        .merge(oauth::routes(store))
        .merge(openapi::routes(store))
        .merge(scripted::routes(store))
        .route_layer(middleware::from_fn_with_state(store.clone(), track::track));
    counted.merge(admin::routes(store)).merge(openapi::switch(store)).layer(CompressionLayer::new())
}
