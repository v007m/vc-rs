use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Json, Router};
use serde_json::json;

use crate::tls::{Names, PeerCert};

/// `GET /whoami` (https mode only): the name given to the client certificate the connection
/// presented (`null` for one without a name), or 403 when it presented none.
pub fn routes(names: Names) -> Router {
    Router::new().route("/whoami", routing::get(whoami)).with_state(names)
}

async fn whoami(State(names): State<Names>, ConnectInfo(PeerCert(cert)): ConnectInfo<PeerCert>) -> Response {
    let Some(cert) = cert else {
        return (StatusCode::FORBIDDEN, Json(json!({ "error": "no client certificate" }))).into_response();
    };
    Json(json!({ "client": names.get(&cert) })).into_response()
}
