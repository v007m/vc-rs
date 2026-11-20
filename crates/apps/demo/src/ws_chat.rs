//! `vc-demo`'s WebSocket chat (D465): the handshake and the replies. The mock's `/ws` has its own
//! few lines of each, since neither crate may depend on a workspace crate.

use std::future::Future;

use axum::extract::Request;
use axum::http::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION, UPGRADE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};

/// The server's side of an upgraded connection.
pub type Socket = WebSocketStream<TokioIo<Upgraded>>;

/// Answers a WebSocket handshake with 101 and hands the upgraded connection, framed under
/// `config`, to `serve`; a request that does not ask to upgrade to version 13 gets 426.
pub fn upgrade<F, Fut>(mut request: Request, config: Option<WebSocketConfig>, serve: F) -> Response
where
    F: FnOnce(Socket) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let Some(accept) = accept(request.headers()) else {
        let version = [(SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"))];
        return (StatusCode::UPGRADE_REQUIRED, version, "GET /ws is a WebSocket").into_response();
    };
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        if let Ok(upgraded) = upgrade.await {
            serve(WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, config).await).await;
        }
    });
    let upgraded =
        [(CONNECTION, HeaderValue::from_static("upgrade")), (UPGRADE, HeaderValue::from_static("websocket"))];
    (StatusCode::SWITCHING_PROTOCOLS, upgraded, [(SEC_WEBSOCKET_ACCEPT, accept)]).into_response()
}

/// The `sec-websocket-accept` for a request that asks to upgrade to `websocket` version 13 with
/// a key (RFC 6455, 4.2.1).
fn accept(headers: &HeaderMap) -> Option<HeaderValue> {
    let version = headers.get(SEC_WEBSOCKET_VERSION).is_some_and(|value| value == "13");
    let websocket = headers.get(UPGRADE).is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let key = headers.get(SEC_WEBSOCKET_KEY).filter(|_| version && websocket)?;
    HeaderValue::try_from(derive_accept_key(key.as_bytes())).ok()
}

/// The replies to a text message: JSON `{"type":"join"}` gets `{"type":"joined","memberId":"m1"}`,
/// and `{"type":"say","text":t}` gets `{"type":"message","text":t}` then `{"type":"seen"}`; any
/// other text comes back as it is.
pub fn replies(text: &str) -> Vec<String> {
    let parsed: Option<Value> = serde_json::from_str(text).ok();
    match parsed.as_ref().and_then(|message| message.get("type")).and_then(Value::as_str) {
        Some("join") => vec![json!({"type": "joined", "memberId": "m1"}).to_string()],
        Some("say") => {
            let said = parsed.as_ref().and_then(|message| message.get("text")).cloned().unwrap_or(Value::Null);
            vec![json!({"type": "message", "text": said}).to_string(), json!({"type": "seen"}).to_string()]
        }
        _ => vec![text.to_owned()],
    }
}
