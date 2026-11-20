//! The demo's `/ws`: the mock's WebSocket chat, capped (D465). A message over [`MAX_MESSAGE`]
//! ends the connection, and the reply past [`MAX_REPLIES`] is a close 1008 instead. Only the
//! binary serves it: the playground's in-process router has no connection to upgrade.

use axum::Router;
use axum::extract::Request;
use axum::response::Response;
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};

use crate::ws_chat::{self, Socket};

/// The largest message `/ws` reads, in bytes.
pub const MAX_MESSAGE: usize = 64 * 1024;

/// The most replies one connection gets.
pub const MAX_REPLIES: usize = 100;

pub fn routes() -> Router {
    Router::new().route("/ws", get(ws))
}

/// `GET /ws` upgrades a WebSocket handshake to the chat; anything else is a 426.
async fn ws(request: Request) -> Response {
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE));
    ws_chat::upgrade(request, Some(config), chat)
}

/// Answers each text message with [`ws_chat::replies`] and sends each binary one back; the text
/// `bye` gets a close 1000 `bye`, and the reply past [`MAX_REPLIES`] a close 1008.
async fn chat(mut socket: Socket) {
    let mut sent = 0;
    while let Some(Ok(message)) = socket.next().await {
        let answers = match message {
            Message::Text(text) if text.as_str() == "bye" => {
                return closed(&mut socket, CloseCode::Normal, "bye").await;
            }
            Message::Text(text) => ws_chat::replies(text.as_str()).into_iter().map(Message::text).collect(),
            Message::Binary(bytes) => vec![Message::Binary(bytes)],
            Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Vec::new(),
        };
        sent += answers.len();
        if sent > MAX_REPLIES {
            return closed(&mut socket, CloseCode::Policy, "too many messages").await;
        }
        if socket.send_all(&mut futures_util::stream::iter(answers.into_iter().map(Ok))).await.is_err() {
            return;
        }
    }
}

/// Closes with `code` and `reason`, and reads until the client's close ends the connection.
async fn closed(socket: &mut Socket, code: CloseCode, reason: &str) {
    if socket.close(Some(CloseFrame { code, reason: reason.into() })).await.is_ok() {
        while let Some(Ok(_)) = socket.next().await {}
    }
}

#[cfg(test)]
#[path = "ws_tests.rs"]
mod tests;
