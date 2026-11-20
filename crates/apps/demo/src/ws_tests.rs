use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::routes::router;
use crate::ws_chat::replies;

fn parsed(replies: &[String]) -> Vec<Value> {
    replies.iter().map(|reply| serde_json::from_str(reply).unwrap()).collect()
}

#[test]
fn join_and_say_get_the_chat_replies_and_other_text_comes_back() {
    assert_eq!(parsed(&replies(r#"{"type":"join"}"#)), [json!({"type": "joined", "memberId": "m1"})]);
    let said = parsed(&replies(r#"{"type":"say","text":"hi"}"#));
    assert_eq!(said, [json!({"type": "message", "text": "hi"}), json!({"type": "seen"})]);
    assert_eq!(replies("hello"), ["hello"]);
    assert_eq!(replies(r#"{"type":"other"}"#), [r#"{"type":"other"}"#]);
}

#[tokio::test]
async fn a_request_that_does_not_ask_to_upgrade_is_426() {
    let request = Request::builder().method(Method::GET).uri("/ws").body(Body::empty()).unwrap();
    let response = router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
}
