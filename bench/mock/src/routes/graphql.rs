use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Json, Router};
use serde_json::Value;

/// `POST /graphql/reply` answers as a GraphQL server told what to say: the request's
/// `variables.reply` as the JSON body, with `variables.status` (200 when unset).
pub fn routes() -> Router {
    Router::new().route("/graphql/reply", routing::post(reply))
}

async fn reply(Json(request): Json<Value>) -> Response {
    let told = &request["variables"];
    let status = told["status"].as_u64().and_then(|code| StatusCode::from_u16(u16::try_from(code).ok()?).ok());
    let status = status.unwrap_or(StatusCode::OK);
    (status, Json(told["reply"].clone())).into_response()
}
