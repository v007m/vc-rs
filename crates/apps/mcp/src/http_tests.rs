use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::{Method, Request, StatusCode, header};
use http_body::{Body, Frame};
use http_body_util::{BodyExt as _, Full};
use serde_json::{Value, json};
use tower_service::Service as _;
use vc_engine::EngineBuilder;

use super::{HttpResponse, HttpService};
use crate::VcServer;
use crate::fixture::project;

/// A body that breaks off before its end.
struct Broken;

impl Body for Broken {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        Poll::Ready(Some(Err(std::io::Error::other("connection reset"))))
    }
}

fn service() -> (tempfile::TempDir, HttpService) {
    let (dir, _engine) = project();
    let engine = EngineBuilder::for_project(dir.path()).unwrap().build().unwrap();
    let (service, _stop) = VcServer::new(engine, vec!["dev".to_owned()], Vec::new()).into_http();
    (dir, service)
}

fn post<B>(body: B) -> Request<B> {
    let request = Request::builder().method(Method::POST).uri("/").header(header::HOST, "127.0.0.1");
    let request = request.header(header::ACCEPT, "application/json, text/event-stream");
    request.header(header::CONTENT_TYPE, "application/json").body(body).unwrap()
}

fn initialize(params: &Value) -> Full<Bytes> {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": params});
    Full::new(Bytes::from(request.to_string()))
}

async fn text(response: HttpResponse) -> String {
    String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
}

#[tokio::test]
async fn an_initialize_without_client_info_or_its_name_opens_a_session() {
    let (_dir, mut service) = service();
    let unnamed = json!({"protocolVersion": "2025-11-25", "capabilities": {}});
    let nameless = json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {}});
    for params in [unnamed, nameless] {
        let opened = service.call(post(initialize(&params))).await.unwrap();
        assert_eq!(opened.status(), StatusCode::OK, "{params}");
        assert!(opened.headers().contains_key("mcp-session-id"), "{params}");
    }
}

#[tokio::test]
async fn any_other_body_reaches_rmcp_as_it_was_sent() {
    let (_dir, mut service) = service();
    let named =
        json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "a", "version": "1"}});
    assert_eq!(service.call(post(initialize(&named))).await.unwrap().status(), StatusCode::OK);
    let garbled = service.call(post(Full::new(Bytes::from_static(b"not json")))).await.unwrap();
    assert_eq!(garbled.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE, "rmcp's own answer");
    let sessionless = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let refused = service.call(post(Full::new(Bytes::from_static(sessionless.as_bytes())))).await.unwrap();
    assert!(refused.status().is_client_error(), "only an initialize opens a session: {}", refused.status());
}

#[tokio::test]
async fn a_body_over_the_limit_or_broken_off_is_refused_without_its_content() {
    let (_dir, mut service) = service();
    service.max = 64;
    let large = service.call(post(Full::new(Bytes::from(vec![b'x'; 65])))).await.unwrap();
    assert_eq!(large.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(text(large).await, "Payload Too Large: request body exceeds 64 bytes");
    let broken = service.call(post(Broken)).await.unwrap();
    assert_eq!(broken.status(), StatusCode::BAD_REQUEST);
    assert_eq!(text(broken).await, "Bad Request: the request body could not be read");
}

#[tokio::test]
async fn a_request_in_a_session_goes_to_rmcp_unread_after_an_unnamed_initialize_opened_it() {
    let (_dir, mut service) = service();
    let unnamed = json!({"protocolVersion": "2025-11-25", "capabilities": {}});
    let opened = service.call(post(initialize(&unnamed))).await.unwrap();
    let session = opened.headers().get("mcp-session-id").cloned().unwrap();
    assert!(text(opened).await.contains("serverInfo"), "the unnamed client's initialize was answered");
    let in_session = |body: &'static str| {
        let mut request = post(Full::new(Bytes::from_static(body.as_bytes())));
        request.headers_mut().insert("mcp-session-id", session.clone());
        request
    };
    let initialized = service.call(in_session(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#));
    assert!(initialized.await.unwrap().status().is_success());
    let listed = service.call(in_session(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)).await.unwrap();
    assert_eq!(listed.status(), StatusCode::OK);
    assert!(text(listed).await.contains("\"tools\""), "the session serves its tools");
    let mut broken = post(Broken);
    broken.headers_mut().insert("mcp-session-id", session);
    let answered = text(service.call(broken).await.unwrap()).await;
    assert_ne!(answered, "Bad Request: the request body could not be read", "rmcp read the body, not the wrapper");
}
