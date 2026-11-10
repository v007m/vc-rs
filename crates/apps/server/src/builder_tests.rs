use std::io::{Read as _, Write as _};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::routing::post;
use bytes::Bytes;
use http_body_util::BodyExt as _;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower::ServiceExt as _;

use crate::fixture::{INDEX_HTML, TestFiles};
use crate::token::Token;
use crate::{ConnectService, ServerBuilder, ServerError, Sink};

async fn fetch(builder: ServerBuilder, method: Method, uri: &str) -> (StatusCode, Option<String>, Bytes) {
    let token = Arc::new(Token::generate().unwrap());
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "127.0.0.1:7420")
        .header(header::AUTHORIZATION, format!("Bearer {}", token.expose()))
        .body(Body::empty())
        .unwrap();
    let response = builder.guarded(token, 7420).oneshot(request).await.unwrap();
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
    assert!(one.session_matches(one.session()) && !one.session_matches(one.expose()) && !one.matches(one.session()));
    assert_ne!(one.session(), two.session());
    assert_eq!(format!("{one:?}"), "Token(<redacted>)");
}

/// A server with one `Wait` stream open, whose handler winds up when told to stop when `heeds`,
/// and otherwise never ends; with the sender that shuts the server down.
async fn one_stream_open(
    builder: ServerBuilder,
    heeds: bool,
) -> (oneshot::Sender<()>, JoinHandle<Result<(), ServerError>>) {
    let wait = move |(): (), sink: Sink<()>| async move {
        if heeds {
            sink.closed().await;
        } else {
            std::future::pending::<()>().await;
        }
        Ok(())
    };
    let server =
        builder.service(ConnectService::new("test.v1.Waiter").server_stream("Wait", wait)).bind().await.unwrap();
    let url = server.launch_url().expose().to_owned();
    let (port, token) = url.strip_prefix("http://127.0.0.1:").unwrap().split_once("/?token=").unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async move { stopped.await.unwrap_or(()) }));
    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await.unwrap();
    let head = format!(
        "POST /api/test.v1.Waiter/Wait HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/connect+proto\r\nContent-Length: 5\r\n\r\n"
    );
    stream.write_all(&[head.as_bytes(), &[0; 5]].concat()).await.unwrap();
    let mut reply = [0; 12];
    stream.read_exact(&mut reply).await.unwrap();
    assert_eq!(&reply, b"HTTP/1.1 200");
    tokio::spawn(async move { stream.read_to_end(&mut Vec::new()).await });
    (stop, serving)
}

#[tokio::test]
async fn shutting_down_ends_the_open_streams_first() {
    let (stop, serving) = one_stream_open(ServerBuilder::new(0), true).await;
    stop.send(()).unwrap();
    let served = tokio::time::timeout(Duration::from_secs(3), serving).await;
    assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
}

#[tokio::test]
async fn a_request_that_outlives_the_grace_period_does_not_hold_the_shutdown() {
    let (stop, serving) = one_stream_open(ServerBuilder::new(0).grace(Duration::from_millis(100)), false).await;
    stop.send(()).unwrap();
    let served = tokio::time::timeout(Duration::from_secs(3), serving).await;
    assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
}

/// A built web UI whose `index.html` has no CSP meta.
struct NoCspMeta;

impl crate::Files for NoCspMeta {
    fn get(&self, path: &str) -> Option<Bytes> {
        (path == "index.html").then(|| Bytes::from_static(b"<!doctype html><script>start()</script>"))
    }
}

#[tokio::test]
async fn a_page_whose_csp_hashes_cannot_be_read_is_reported_and_gets_the_strict_header() {
    assert_eq!(site().bind().await.unwrap().csp_problem(), None);
    assert_eq!(ServerBuilder::new(0).bind().await.unwrap().csp_problem(), None);
    let builder = || ServerBuilder::new(0).files(Arc::new(NoCspMeta));
    assert!(builder().bind().await.unwrap().csp_problem().is_some_and(|problem| problem.contains("index.html")));
    let token = Arc::new(Token::generate().unwrap());
    let request = Request::builder().uri("/").header(header::HOST, "127.0.0.1:7420").body(Body::empty()).unwrap();
    let response = builder().guarded(token, 7420).oneshot(request).await.unwrap();
    let csp = response.headers().get(header::CONTENT_SECURITY_POLICY).map(|value| value.to_str().unwrap().to_owned());
    assert_eq!(csp.as_deref(), Some(crate::csp::FALLBACK));
    assert!(csp.is_some_and(|csp| csp.contains("frame-ancestors 'none'")), "the fallback still forbids framing");
}
