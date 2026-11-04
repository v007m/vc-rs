mod admin;
mod auth;
mod crud;
mod echo;
mod scripted;
mod track;

use axum::Router;
use axum::middleware;
use tower_http::compression::CompressionLayer;

use crate::store::Store;

/// Builds the app. Every route except `/__stats` and `/__reset` is counted in [`crate::Stats`].
/// Responses are gzip- or brotli-compressed when the client's `Accept-Encoding` allows it.
pub fn router(store: &Store) -> Router {
    let counted = Router::new()
        .merge(crud::routes(store, "users"))
        .merge(crud::routes(store, "orders"))
        .merge(auth::routes(store))
        .merge(echo::routes())
        .merge(scripted::routes(store))
        .route_layer(middleware::from_fn_with_state(store.clone(), track::track));
    counted.merge(admin::routes(store)).layer(CompressionLayer::new())
}
