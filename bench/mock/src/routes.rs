mod admin;
mod auth;
mod aws;
mod crud;
mod echo;
mod events;
mod graphql;
mod oauth;
mod openapi;
mod owned;
mod scripted;
mod track;
mod whoami;
mod ws;

use axum::Router;
use axum::middleware;
use tower_http::compression::CompressionLayer;

use crate::store::Store;

pub use aws::{ACCESS_KEY_ID as AWS_ACCESS_KEY_ID, SECRET_ACCESS_KEY as AWS_SECRET_ACCESS_KEY};
pub use whoami::routes as whoami;

/// Builds the app, with `extra` routes beside the usual ones. Every route except `/__stats`, `/__reset` and `/__openapi/…` is counted in
/// [`crate::Stats`].
/// Responses are gzip- or brotli-compressed when the client's `Accept-Encoding` allows it.
pub fn router(store: &Store, extra: Router) -> Router {
    let counted = Router::new()
        .merge(crud::routes(store, "users"))
        .merge(crud::routes(store, "orders"))
        .merge(owned::routes(store))
        .merge(auth::routes(store))
        .merge(aws::routes())
        .merge(echo::routes())
        .merge(events::routes())
        .merge(graphql::routes())
        .merge(oauth::routes(store))
        .merge(openapi::routes(store))
        .merge(scripted::routes(store))
        .merge(ws::routes())
        .merge(extra)
        .route_layer(middleware::from_fn_with_state(store.clone(), track::track));
    counted.merge(admin::routes(store)).merge(openapi::switch(store)).layer(CompressionLayer::new())
}
