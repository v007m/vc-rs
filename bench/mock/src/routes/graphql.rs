use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Json, Router};
use serde_json::Value;

use super::echo;

/// `POST /graphql/reply` answers as a GraphQL server told what to say: the request's
/// `variables.reply` as the JSON body, with `variables.status` (200 when unset). `POST
/// /graphql/echo` and `/graphql/echo/…` answer as `/echo` does, with `data: null` beside, so the
/// answer is a GraphQL response.
pub fn routes() -> Router {
    Router::new()
        .route("/graphql/reply", routing::post(reply))
        .route("/graphql/echo", routing::post(echoed))
        .route("/graphql/echo/{*rest}", routing::post(echoed))
}

async fn echoed(request: Request) -> Response {
    echo::described(request, |mut echo| {
        if let Value::Object(fields) = &mut echo {
            drop(fields.insert("data".to_owned(), Value::Null));
        }
        echo
    })
    .await
}

async fn reply(Json(request): Json<Value>) -> Response {
    let told = &request["variables"];
    let status = told["status"].as_u64().and_then(|code| StatusCode::from_u16(u16::try_from(code).ok()?).ok());
    let status = status.unwrap_or(StatusCode::OK);
    (status, Json(told["reply"].clone())).into_response()
}
