//! `CallStream` streams a live call's events as they come, masked, and `StreamSend` closes it;
//! a stream that is not open, or a send with nothing in it, is refused (D543, D544).

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use bytes::{Buf as _, Bytes, BytesMut};
use http_body_util::BodyExt as _;
use prost::Message as _;
use serde_json::{Value, json};
use tower::ServiceExt as _;
use vc_contracts::engine::v1 as wire;
use vc_contracts::engine::v1::call_stream_response::Event;
use vc_contracts::results::v1::Header;

use crate::engine_fixture::{SECRET, project};
use crate::envelope;

const SERVICE: &str = "/api/vibranium.engine.v1.EngineService";

/// A server on a free local port whose every answer is an event stream that sends one event,
/// then stays open until the client goes; its base URL.
fn ticking() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for mut socket in listener.incoming().flatten() {
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request);
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
            let _ = socket.write_all(format!("{head}event: tick\ndata: 1\n\n").as_bytes());
            while socket.read(&mut request).is_ok_and(|read| read > 0) {}
        }
    });
    url
}

/// The service's router over the fixture project.
fn router() -> (tempfile::TempDir, Router) {
    let (dir, api) = project();
    (dir, Arc::into_inner(api).unwrap().into_service().into_router())
}

/// An ad hoc `sse` call of `url`, its secret in a header.
fn ticks(url: &str) -> wire::CallStreamRequest {
    let adhoc = wire::AdhocCall {
        r#type: "sse".into(),
        url: format!("{url}/ticks"),
        headers: vec![Header { name: "x-key".into(), value: "${secret.SHOP_TOKEN}".into() }],
        ..wire::AdhocCall::default()
    };
    wire::CallStreamRequest { call: Some(wire::CallRequest { adhoc: Some(adhoc), ..wire::CallRequest::default() }) }
}

/// The body of the stream `CallStream` answers `request` with.
async fn opened(router: &Router, request: &wire::CallStreamRequest) -> Body {
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{SERVICE}/CallStream"))
        .header(header::CONTENT_TYPE, "application/connect+proto")
        .body(Body::from(envelope::message(&request.encode_to_vec())))
        .unwrap();
    router.clone().oneshot(request).await.unwrap().into_body()
}

/// The stream's next envelope, read as it arrives: its flags and payload; everything read is
/// kept in `seen`.
async fn envelope(body: &mut Body, (buffer, seen): (&mut BytesMut, &mut Vec<u8>)) -> (u8, Bytes) {
    loop {
        if let [flags, a, b, c, d, ..] = buffer[..] {
            let length = usize::try_from(u32::from_be_bytes([a, b, c, d])).unwrap();
            if buffer.len() >= 5 + length {
                buffer.advance(5);
                return (flags, buffer.split_to(length).freeze());
            }
        }
        let frame = body.frame().await.unwrap().unwrap().into_data().unwrap();
        seen.extend_from_slice(&frame);
        buffer.extend_from_slice(&frame);
    }
}

/// The stream's next event.
async fn event(body: &mut Body, read: (&mut BytesMut, &mut Vec<u8>)) -> Event {
    let (flags, payload) = envelope(body, read).await;
    assert_eq!(flags, 0, "{}", String::from_utf8_lossy(&payload));
    wire::CallStreamResponse::decode(payload).unwrap().event.unwrap()
}

/// `StreamSend`'s answer to `request`, as JSON: its status and body.
async fn send(router: &Router, request: &Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::POST)
        .uri(format!("{SERVICE}/StreamSend"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(request.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_event_stream_streams_its_events_masked_refuses_a_text_and_closes_on_stream_send() {
    let (_dir, router) = router();
    let mut body = opened(&router, &ticks(&ticking())).await;
    let (mut buffer, mut seen) = (BytesMut::new(), Vec::new());
    let Event::Opened(open) = event(&mut body, (&mut buffer, &mut seen)).await else { panic!("opened first") };
    let head = open.head.clone().unwrap();
    assert!(open.stream.starts_with("live-") && head.url.ends_with("/ticks") && !open.sends, "{open:?}");
    assert_eq!(head.status, Some(200));
    let Event::Message(message) = event(&mut body, (&mut buffer, &mut seen)).await else { panic!("a message") };
    assert_eq!(message.direction, "in");
    assert_eq!(serde_json::from_str::<Value>(&message.json).unwrap()["data"], json!(1));
    let (status, error) = send(&router, &json!({"stream": open.stream, "text": "hi"})).await;
    assert_eq!((status, error["code"].as_str()), (StatusCode::BAD_REQUEST, Some("failed_precondition")));
    assert!(error["message"].as_str().unwrap().contains("only reads"), "{error}");
    assert_eq!(send(&router, &json!({"stream": open.stream, "close": true})).await, (StatusCode::OK, json!({})));
    let Event::Ended(ended) = event(&mut body, (&mut buffer, &mut seen)).await else { panic!("ended last") };
    assert_eq!((ended.reason.as_str(), ended.failed), ("the client closed the stream", false));
    let (flags, end) = envelope(&mut body, (&mut buffer, &mut seen)).await;
    assert_eq!((flags, serde_json::from_slice::<Value>(&end).unwrap()), (2, json!({})));
    assert!(!seen.windows(SECRET.len()).any(|found| found == SECRET.as_bytes()), "the secret is masked");
    let (status, _) = send(&router, &json!({"stream": open.stream, "text": "late"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a stream that ended is gone");
}

#[tokio::test(flavor = "multi_thread")]
async fn stream_send_refuses_a_stream_that_is_not_open_and_a_send_with_nothing_in_it() {
    let (_dir, router) = router();
    let (status, error) = send(&router, &json!({"stream": "live-9", "text": "hi"})).await;
    assert_eq!((status, error["code"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));
    let (status, _) = send(&router, &json!({"stream": "live-9"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_stream_that_cannot_open_ends_with_its_reason() {
    let (_dir, router) = router();
    let request = wire::CallStreamRequest {
        call: Some(wire::CallRequest { api: "users#list".into(), ..wire::CallRequest::default() }),
    };
    let mut body = opened(&router, &request).await;
    let (mut buffer, mut seen) = (BytesMut::new(), Vec::new());
    let Event::Ended(ended) = event(&mut body, (&mut buffer, &mut seen)).await else { panic!("only ended") };
    assert!(ended.failed && ended.reason.contains("`sse` or `websocket`"), "{ended:?}");
}
