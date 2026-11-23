//! `/ws`: the stream suite's WebSocket echo and the chat replies (D465).

mod support;

use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use support::{Mock, TestResult};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

#[tokio::test]
async fn ws_echoes_answers_the_chat_and_closes_on_bye() -> TestResult {
    let mock = Mock::start().await?;
    let base = mock.handle.base_url();
    let address = base.trim_start_matches("http://");
    let tcp = tokio::net::TcpStream::connect(address).await?;
    let (mut socket, answer) = tokio_tungstenite::client_async(format!("ws://{address}/ws"), tcp).await?;
    assert_eq!(answer.status(), StatusCode::SWITCHING_PROTOCOLS);
    socket.send(Message::text("hi")).await?;
    socket.send(Message::binary(vec![0, 255])).await?;
    socket.send(Message::text(r#"{"type":"join"}"#)).await?;
    socket.send(Message::text(r#"{"type":"say","text":"hi"}"#)).await?;
    socket.send(Message::text("bye")).await?;
    let mut read = Vec::new();
    while let Some(Ok(message)) = socket.next().await {
        read.push(message);
    }
    let close = Message::Close(Some(CloseFrame { code: CloseCode::Normal, reason: "bye".into() }));
    let expected = [
        Message::text("hi"),
        Message::binary(vec![0, 255]),
        Message::text(r#"{"memberId":"m1","type":"joined"}"#),
        Message::text(r#"{"text":"hi","type":"message"}"#),
        Message::text(r#"{"type":"seen"}"#),
        close,
    ];
    assert_eq!(read, expected);
    assert_eq!(mock.handle.stats().await?.count("GET /ws"), 1);
    Ok(())
}

#[tokio::test]
async fn ws_without_an_upgrade_is_426() -> TestResult {
    let mock = Mock::start().await?;
    assert_eq!(mock.get("/ws").send().await?.status(), StatusCode::UPGRADE_REQUIRED);
    Ok(())
}
