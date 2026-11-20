//! `GET /ws`: the stream suite's WebSocket echo, which is `vc-demo`'s chat uncapped (D465).

use axum::Router;
use axum::extract::Request;
use axum::response::Response;
use axum::routing;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

#[path = "../../../../crates/apps/demo/src/ws_chat.rs"]
mod chat;

pub fn routes() -> Router {
    Router::new().route("/ws", routing::get(ws))
}

/// Upgrades a WebSocket handshake to the echo; anything else is a 426.
async fn ws(request: Request) -> Response {
    chat::upgrade(request, None, echo)
}

/// Answers each text message with [`chat::replies`] and sends each binary one back, and answers
/// the text `bye` with a close 1000 `bye`; ends when the connection does.
async fn echo(mut socket: chat::Socket) {
    while let Some(Ok(message)) = socket.next().await {
        let sent = match message {
            Message::Text(text) if text.as_str() == "bye" => {
                socket.close(Some(CloseFrame { code: CloseCode::Normal, reason: "bye".into() })).await
            }
            Message::Text(text) => {
                let replies = chat::replies(text.as_str()).into_iter().map(Message::text).map(Ok);
                socket.send_all(&mut futures_util::stream::iter(replies)).await
            }
            Message::Binary(_) => socket.send(message).await,
            Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => Ok(()),
        };
        if sent.is_err() {
            break;
        }
    }
}
