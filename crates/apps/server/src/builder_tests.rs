use std::io::{Read as _, Write as _};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::routing::post;
use bytes::Bytes;
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;

use crate::ServerBuilder;
use crate::fixture::{INDEX_HTML, TestFiles};
use crate::token::Token;

async fn fetch(builder: ServerBuilder, method: Method, uri: &str) -> (StatusCode, Option<String>, Bytes) {
    let token = Arc::new(Token::generate().unwrap());
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7420")
        .header(header::AUTHORIZATION, format!("Bearer {}", token.expose()))
        .body(Body::empty())
        .unwrap();
    let response = builder.app(token, 7420).oneshot(request).await.unwrap();
    let content_type = response.headers().get(header::CONTENT_TYPE).map(|v| v.to_str().unwrap().to_owned());
    (response.status(), content_type, response.into_body().collect().await.unwrap().to_bytes())
}

fn site() -> ServerBuilder {
    ServerBuilder::new(0).files(Arc::new(TestFiles))
}

#[tokio::test]
async fn serves_the_files_and_gives_the_app_every_other_page() {
    let (status, media, body) = fetch(site(), Method::GET, "/_app/immutable/app.js").await;
    assert_eq!(
        (status, media.as_deref(), body.as_ref()),
        (StatusCode::OK, Some("text/javascript; charset=utf-8"), &b"export {};"[..])
    );
    for page in ["/", "/runs/12", "/editor/services/users"] {
        let (status, media, body) = fetch(site(), Method::GET, page).await;
        assert_eq!((status, media.as_deref()), (StatusCode::OK, Some("text/html; charset=utf-8")), "{page}");
        assert_eq!(body, INDEX_HTML.as_bytes());
    }
}

#[tokio::test]
async fn a_missing_file_an_unknown_api_or_a_write_is_not_found() {
    for (method, uri) in [
        (Method::GET, "/_app/immutable/missing.js"),
        (Method::GET, "/api/test.v1.Other/Method"),
        (Method::POST, "/runs"),
    ] {
        assert_eq!(fetch(site(), method, uri).await.0, StatusCode::NOT_FOUND, "{uri}");
    }
    assert_eq!(fetch(ServerBuilder::new(0), Method::GET, "/").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_mount_answers_under_its_path_behind_the_guard() {
    let mcp = Router::new().route("/", post(|| async { "mcp" }));
    let builder = site().mount("/mcp", mcp);
    let (status, _, body) = fetch(builder, Method::POST, "/mcp").await;
    assert_eq!((status, body.as_ref()), (StatusCode::OK, &b"mcp"[..]));
}

#[tokio::test]
async fn listens_on_loopback_and_shows_the_token_only_in_the_launch_url() {
    let server = ServerBuilder::new(0).bind().await.unwrap();
    let url = server.launch_url().expose().to_owned();
    let rest = url.strip_prefix("http://127.0.0.1:").unwrap();
    let (port, token) = rest.split_once("/?token=").unwrap();
    assert!(port.parse::<u16>().is_ok_and(|port| port != 0));
    assert_eq!(token.len(), 43);
    assert!(token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    assert_eq!(format!("{:?}", server.launch_url()), "LaunchUrl(<redacted>)");

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async move { stopped.await.unwrap_or(()) }));
    let request = format!("GET /?token={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let address = format!("127.0.0.1:{port}");
    let reply = tokio::task::spawn_blocking(move || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    })
    .await
    .unwrap();
    assert!(reply.starts_with("HTTP/1.1 303"), "{reply}");
    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();
}

#[test]
fn tokens_are_random_compared_whole_and_hidden_from_debug() {
    let (one, two) = (Token::generate().unwrap(), Token::generate().unwrap());
    assert_ne!(one.expose(), two.expose());
    assert!(one.matches(one.expose()));
    assert!(!one.matches(two.expose()));
    assert!(!one.matches(&one.expose()[..42]));
    assert!(!one.matches(""));
    assert_eq!(format!("{one:?}"), "Token(<redacted>)");
}
