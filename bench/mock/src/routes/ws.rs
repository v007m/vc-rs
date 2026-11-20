//! `GET /ws`: the stream suite's WebSocket echo, plus the chat replies its tests read (D465).
//! The handshake is the mock's own few lines: axum's `ws` feature would add tungstenite 0.29,
//! sha1 0.10 and rand 0.9 beside the versions the workspace already has.

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use axum::response::{IntoResponse, Response};
use axum::routing;
use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Role};

pub fn routes() -> Router {
    Router::new().route("/ws", routing::get(ws))
}

/// Answers a request with a `sec-websocket-key` with 101 and runs [`echo`] on the upgraded
/// connection; any other request gets 426.
async fn ws(mut request: Request) -> Response {
    let Some(key) = request.headers().get(SEC_WEBSOCKET_KEY) else {
        return (StatusCode::UPGRADE_REQUIRED, "GET /ws is a WebSocket").into_response();
    };
    let accept = derive_accept_key(key.as_bytes());
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        if let Ok(upgraded) = upgrade.await {
            echo(WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None).await).await;
        }
    });
    let switched =
        [(CONNECTION, "upgrade".to_owned()), (UPGRADE, "websocket".to_owned()), (SEC_WEBSOCKET_ACCEPT, accept)];
    (StatusCode::SWITCHING_PROTOCOLS, switched).into_response()
}

/// Answers each text message with [`replies`] and sends each binary one back, and answers the
/// text `bye` with a close 1000 `bye`; ends when the connection does.
async fn echo(mut socket: WebSocketStream<TokioIo<Upgraded>>) {
    while let Some(Ok(message)) = socket.next().await {
        let sent = match message {
            Message::Text(text) if text.as_str() == "bye" => {
                socket.close(Some(CloseFrame { code: CloseCode::Normal, reason: "bye".into() })).await
            }
            Message::Text(text) => {
                let answers = replies(text.as_str()).into_iter().map(Message::text).map(Ok);
                socket.send_all(&mut futures_util::stream::iter(answers)).await
            }
            Message::Binary(_) => socket.send(message).await,
            Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(()),
        };
        if sent.is_err() {
            break;
        }
    }
}

/// `{"type":"join"}` gets `{"type":"joined","memberId":"m1"}`; `{"type":"say","text":t}` gets
/// `{"type":"message","text":t}` then `{"type":"seen"}`; any other text comes back as it is.
fn replies(text: &str) -> Vec<String> {
    let message: Value = serde_json::from_str(text).unwrap_or_default();
    match message["type"].as_str() {
        Some("join") => vec![json!({"type": "joined", "memberId": "m1"}).to_string()],
        Some("say") => {
            vec![json!({"type": "message", "text": message["text"]}).to_string(), json!({"type": "seen"}).to_string()]
        }
        _ => vec![text.to_owned()],
    }
}
