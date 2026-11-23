use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::json;
use tower::ServiceExt;

use super::{MAX_EVENTS, MAX_RETRY_MS};
use crate::routes::router;
use crate::testing::send;

/// The status, content type and body text of `GET path` (with `last-event-id` when given), and
/// whether the body ended in an error.
async fn events(path: &str, last_event_id: Option<&str>) -> (StatusCode, String, String, bool) {
    let mut request = Request::builder().method(Method::GET).uri(path);
    if let Some(id) = last_event_id {
        request = request.header("last-event-id", id);
    }
    let response = router().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let kind = response.headers().get(header::CONTENT_TYPE).map(|value| value.to_str().unwrap().to_owned());
    let mut stream = response.into_body().into_data_stream();
    let (mut text, mut failed) = (String::new(), false);
    while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
        match chunk {
            Ok(bytes) => text.push_str(std::str::from_utf8(&bytes).unwrap()),
            Err(_) => failed = true,
        }
    }
    (status, kind.unwrap_or_default(), text, failed)
}

#[tokio::test]
async fn events_send_the_suite_body_then_end() {
    let (status, kind, text, failed) = events("/events?count=2", None).await;
    assert_eq!((status, kind.as_str(), failed), (StatusCode::OK, "text/event-stream", false));
    assert_eq!(text, "id: 1\ndata: 1\n\nid: 2\ndata: 2\n\n");
    assert_eq!(events("/events", None).await.2.matches("data:").count(), 3);
}

#[tokio::test]
async fn events_resume_after_the_last_event_id() {
    assert_eq!(events("/events?count=3", Some("1")).await.2, "id: 2\ndata: 2\n\nid: 3\ndata: 3\n\n");
    assert_eq!(events("/events?count=1", Some("soon")).await.2, "id: 1\ndata: 1\n\n");
}

#[tokio::test]
async fn drop_and_drop_at_end_the_stream_in_an_error() {
    let (_, _, text, failed) = events("/events?count=1&drop=true", None).await;
    assert_eq!((text.as_str(), failed), ("id: 1\ndata: 1\n\n", true));
    let (_, _, text, failed) = events("/events?count=3&dropAt=1", None).await;
    assert_eq!((text.as_str(), failed), ("id: 1\ndata: 1\n\n", true));
    let (_, _, text, failed) = events("/events?count=3&dropAt=1", Some("1")).await;
    assert_eq!((text.as_str(), failed), ("id: 2\ndata: 2\n\nid: 3\ndata: 3\n\n", false));
}

#[tokio::test]
async fn retry_and_heartbeat_are_opt_in() {
    let text = events("/events?count=1&retry=250&heartbeat=true", None).await.2;
    assert_eq!(text, "retry: 250\n\n: heartbeat\nid: 1\ndata: 1\n\n");
}

#[tokio::test]
async fn events_refuse_a_count_or_retry_over_the_cap_and_a_bad_query() {
    let app = router();
    let cases = [
        (format!("/events?count={}", MAX_EVENTS + 1), format!("`count` is at most {MAX_EVENTS}")),
        (format!("/events?retry={}", MAX_RETRY_MS + 1), format!("`retry` is at most {MAX_RETRY_MS}")),
        ("/events?count=-1".to_owned(), "`count` is a whole number".to_owned()),
        ("/events?dropAt=x".to_owned(), "`dropAt` is a whole number".to_owned()),
        ("/events?drop=yes".to_owned(), "`drop` is true or false".to_owned()),
        ("/events?heartbeat=1".to_owned(), "`heartbeat` is true or false".to_owned()),
    ];
    for (path, error) in cases {
        assert_eq!(
            send(&app, Method::GET, &path, "").await,
            (StatusCode::BAD_REQUEST, json!({"error": error})),
            "{path}"
        );
    }
    let (status, _, text, _) = events(&format!("/events?count={MAX_EVENTS}"), None).await;
    assert_eq!((status, u64::try_from(text.matches("data:").count()).unwrap()), (StatusCode::OK, MAX_EVENTS));
}
