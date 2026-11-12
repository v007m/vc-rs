use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing;

use crate::error::MockError;
use crate::store::Store;

/// The users spec before and after a backend change: v2 adds `limit` to `listUsers`, a
/// `role` field and `updateUser`, and drops `deleteUser`. Both are `OpenAPI` 3.0, so an
/// import also converts them to 3.1.
const SPECS: [&str; 2] = [include_str!("../../specs/users-v1.yaml"), include_str!("../../specs/users-v2.yaml")];

/// The `servers` URL in the spec files, replaced by the address the request was sent to.
const SERVER: &str = "http://localhost:18080";

/// The only `authorization` `/openapi/private/users.yaml` serves the spec to.
const SPEC_READER: &str = "Bearer spec-reader";

/// `GET /openapi/users-v1.yaml` and `GET /openapi/users-v2.yaml` serve one version each;
/// `GET /openapi/users.yaml` serves the current one, v1 until `POST /__openapi/users/v2`, and
/// `GET /openapi/private/users.yaml` serves it too, but only with [`SPEC_READER`].
pub fn routes(store: &Store) -> Router {
    Router::new()
        .route("/openapi/users.yaml", routing::get(current))
        .route("/openapi/private/users.yaml", routing::get(private))
        .route("/openapi/users-v1.yaml", routing::get(|headers: HeaderMap| async move { spec(0, &headers) }))
        .route("/openapi/users-v2.yaml", routing::get(|headers: HeaderMap| async move { spec(1, &headers) }))
        .with_state(store.clone())
}

/// `POST /__openapi/users/{v1|v2}` picks the version `/openapi/users.yaml` serves. Not counted.
pub fn switch(store: &Store) -> Router {
    Router::new().route("/__openapi/users/{version}", routing::post(pick)).with_state(store.clone())
}

async fn current(State(store): State<Store>, headers: HeaderMap) -> Result<Response, MockError> {
    let version = store.call(|state| state.spec_version).await?;
    Ok(spec(version, &headers))
}

async fn private(store: State<Store>, headers: HeaderMap) -> Result<Response, MockError> {
    if headers.get(header::AUTHORIZATION).is_none_or(|value| value != SPEC_READER) {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    current(store, headers).await
}

async fn pick(State(store): State<Store>, Path(version): Path<String>) -> Result<StatusCode, MockError> {
    let Some(index) = ["v1", "v2"].iter().position(|name| *name == version) else {
        return Ok(StatusCode::NOT_FOUND);
    };
    store.call(move |state| state.spec_version = index).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn spec(version: usize, headers: &HeaderMap) -> Response {
    let host = headers.get(header::HOST).and_then(|host| host.to_str().ok()).unwrap_or("localhost:18080");
    let text = SPECS.get(version).copied().unwrap_or_default().replace(SERVER, &format!("http://{host}"));
    ([(header::CONTENT_TYPE, "application/yaml")], text).into_response()
}
