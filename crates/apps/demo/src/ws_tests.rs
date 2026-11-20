use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tower::ServiceExt;

use super::MAX_REPLIES;
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

#[tokio::test]
async fn another_websocket_version_is_426_naming_13() {
    let request = Request::builder()
        .uri("/ws")
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "8")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(Body::empty())
        .unwrap();
    let response = router().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    assert_eq!(response.headers()["sec-websocket-version"], "13");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hundredth_reply_is_sent_and_the_next_is_a_close_1008() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router()).await });
    let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
    let (mut socket, _) = tokio_tungstenite::client_async(format!("ws://{address}/ws"), tcp).await.unwrap();
    for _ in 1..MAX_REPLIES {
        socket.send(Message::text("x")).await.unwrap();
    }
    socket.send(Message::text(r#"{"type":"say","text":"hi"}"#)).await.unwrap();
    let mut read = Vec::new();
    while let Some(Ok(message)) = socket.next().await {
        read.push(message);
    }
    assert_eq!(read.len(), MAX_REPLIES + 1, "99 echoes, the say's first reply, and the close");
    assert_eq!(read[MAX_REPLIES - 1], Message::text(r#"{"text":"hi","type":"message"}"#));
    let Message::Close(Some(close)) = &read[MAX_REPLIES] else { panic!("{:?}", read[MAX_REPLIES]) };
    assert_eq!(close.code, CloseCode::Policy);
}
