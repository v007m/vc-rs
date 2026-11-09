use axum::body::{self, Body};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use super::router;

/// The status and JSON body (`null` when empty) of `method path` with `body`.
async fn send(app: &axum::Router, method: Method, path: &str, body: &str) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(path).body(Body::from(body.to_owned())).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn users_are_listed_read_created_and_removed() {
    let app = router();
    let (status, all) = send(&app, Method::GET, "/users", "").await;
    assert_eq!((status, all[1]["name"].clone()), (StatusCode::OK, json!("Grace Hopper")));
    assert_eq!(send(&app, Method::GET, "/users/1", "").await.1["name"], "Ada Lovelace");
    assert_eq!(send(&app, Method::GET, "/users/9", "").await.0, StatusCode::NOT_FOUND);
    let created = send(&app, Method::POST, "/users", r#"{"name": "Alan"}"#).await;
    assert_eq!(created, (StatusCode::CREATED, json!({"id": 3, "name": "Alan"})));
    assert_eq!(send(&app, Method::POST, "/users", "").await.1, json!({"id": 4, "name": "someone"}));
    assert_eq!(send(&app, Method::DELETE, "/users/3", "").await, (StatusCode::NO_CONTENT, Value::Null));
}

#[tokio::test]
async fn echo_describes_the_request() {
    let app = router();
    let (status, echoed) = send(&app, Method::PUT, "/echo?a=1", r#"{"word": "hi"}"#).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!((echoed["method"].clone(), echoed["path"].clone()), (json!("PUT"), json!("/echo")));
    assert_eq!((echoed["query"]["a"].clone(), echoed["body"]["word"].clone()), (json!("1"), json!("hi")));
    assert_eq!(send(&app, Method::GET, "/echo", "plain").await.1["body"], "plain");
    assert_eq!(send(&app, Method::GET, "/echo", "").await.1["body"], Value::Null);
}
