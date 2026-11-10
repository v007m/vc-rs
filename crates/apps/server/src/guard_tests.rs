use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use bytes::Bytes;
use http_body_util::BodyExt as _;
use tower::ServiceExt as _;
use vc_contracts::engine::v1::{ReadFileRequest, ReadFileResponse};

use crate::fixture::{INDEX_HTML, TestFiles};
use crate::token::Token;
use crate::{ConnectService, ServerBuilder};

const HOST: &str = "127.0.0.1:7420";
const READ_FILE: &str = "/api/test.v1.TestService/ReadFile";

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

async fn send(token: &Arc<Token>, request: Request<Body>) -> Reply {
    let service = ConnectService::new("test.v1.TestService")
        .unary("ReadFile", |_: ReadFileRequest| async { Ok(ReadFileResponse::default()) });
    let app = ServerBuilder::new(0).service(service).files(Arc::new(TestFiles)).guarded(Arc::clone(token), 7420);
    let response = app.oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    Reply { status: parts.status, headers: parts.headers, body: body.collect().await.unwrap().to_bytes() }
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::builder().method(Method::GET).uri(uri).header(header::HOST, HOST)
}

fn read_file() -> axum::http::request::Builder {
    Request::builder()
        .method(Method::POST)
        .uri(READ_FILE)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/proto")
}

fn token() -> Arc<Token> {
    Arc::new(Token::generate().unwrap())
}

#[tokio::test]
async fn the_launch_url_sets_the_cookie_and_redirects_without_the_token() {
    let token = token();
    let reply = send(&token, get(&format!("/?token={}", token.expose())).body(Body::empty()).unwrap()).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.headers[header::LOCATION], "/");
    let cookie = reply.headers[header::SET_COOKIE].to_str().unwrap();
    assert_eq!(cookie, format!("vc_token={}; Path=/; HttpOnly; SameSite=Strict", token.session()));
    assert_ne!(token.session(), token.expose());
    assert!(reply.headers[header::SET_COOKIE].is_sensitive());
}

#[tokio::test]
async fn a_wrong_launch_token_signs_nobody_in_and_is_not_echoed() {
    let token = token();
    let reply = send(&token, get("/?token=not-the-token-0123456789").body(Body::empty()).unwrap()).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(!reply.headers.contains_key(header::SET_COOKIE));
    assert!(!String::from_utf8_lossy(&reply.body).contains("not-the-token"));
}

#[tokio::test]
async fn the_token_in_any_other_url_is_refused() {
    let token = token();
    let bearer = format!("Bearer {}", token.expose());
    for uri in [format!("/runs?token={}", token.expose()), format!("/?a=1&token={}", token.expose())] {
        let request = get(&uri).header(header::AUTHORIZATION, &bearer).body(Body::empty()).unwrap();
        let expected = if uri.starts_with("/?") { StatusCode::SEE_OTHER } else { StatusCode::BAD_REQUEST };
        assert_eq!(send(&token, request).await.status, expected, "{uri}");
    }
    let post = read_file().uri(format!("{READ_FILE}?token={}", token.expose())).header(header::AUTHORIZATION, &bearer);
    assert_eq!(send(&token, post.body(Body::empty()).unwrap()).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_cookie_or_a_bearer_token_lets_a_request_through() {
    let token = token();
    let cookie = format!("theme=dark; vc_token={}", token.session());
    let page = send(&token, get("/runs/12").header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await;
    assert_eq!((page.status, page.body.as_ref()), (StatusCode::OK, INDEX_HTML.as_bytes()));
    let call = read_file().header(header::AUTHORIZATION, format!("bearer {}", token.expose()));
    assert_eq!(send(&token, call.body(Body::empty()).unwrap()).await.status, StatusCode::OK);
}

#[tokio::test]
async fn a_request_without_the_token_is_unauthorized() {
    let token = token();
    let requests = [
        get("/").body(Body::empty()).unwrap(),
        get("/").header(header::COOKIE, "vc_token=guess").body(Body::empty()).unwrap(),
        read_file().header(header::AUTHORIZATION, "Bearer guess").body(Body::empty()).unwrap(),
        read_file().header(header::AUTHORIZATION, token.expose()).body(Body::empty()).unwrap(),
    ];
    for request in requests {
        assert_eq!(send(&token, request).await.status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn the_cookie_and_the_bearer_token_are_never_interchangeable() {
    let token = token();
    let launch_in_cookie = get("/").header(header::COOKIE, format!("vc_token={}", token.expose()));
    assert_eq!(send(&token, launch_in_cookie.body(Body::empty()).unwrap()).await.status, StatusCode::UNAUTHORIZED);
    let session_as_bearer = read_file().header(header::AUTHORIZATION, format!("Bearer {}", token.session()));
    assert_eq!(send(&token, session_as_bearer.body(Body::empty()).unwrap()).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_write_signed_in_only_by_the_cookie_must_carry_our_origin() {
    let token = token();
    let cookie = format!("vc_token={}", token.session());
    let without = read_file().header(header::COOKIE, &cookie);
    assert_eq!(send(&token, without.body(Body::empty()).unwrap()).await.status, StatusCode::FORBIDDEN);
    let ours = read_file().header(header::COOKIE, &cookie).header(header::ORIGIN, "http://127.0.0.1:7420");
    assert_eq!(send(&token, ours.body(Body::empty()).unwrap()).await.status, StatusCode::OK);
    let bearer = read_file().header(header::AUTHORIZATION, format!("Bearer {}", token.expose()));
    assert_eq!(send(&token, bearer.body(Body::empty()).unwrap()).await.status, StatusCode::OK);
}

#[tokio::test]
async fn only_this_servers_host_names_are_answered() {
    let token = token();
    let cookie = format!("vc_token={}", token.session());
    for (host, expected) in [
        ("localhost:7420", StatusCode::OK),
        ("LOCALHOST:7420", StatusCode::OK),
        ("evil.example:7420", StatusCode::FORBIDDEN),
        ("127.0.0.1:7421", StatusCode::FORBIDDEN),
        ("127.0.0.1", StatusCode::FORBIDDEN),
    ] {
        let mut request = get("/").header(header::COOKIE, &cookie).body(Body::empty()).unwrap();
        request.headers_mut().insert(header::HOST, host.parse().unwrap());
        assert_eq!(send(&token, request).await.status, expected, "{host}");
    }
}

#[tokio::test]
async fn a_write_from_another_sites_page_is_forbidden() {
    let token = token();
    let bearer = format!("Bearer {}", token.expose());
    for (origin, expected) in [
        (Some("http://evil.example"), StatusCode::FORBIDDEN),
        (Some("http://127.0.0.1:7421"), StatusCode::FORBIDDEN),
        (Some("null"), StatusCode::FORBIDDEN),
        (Some("http://localhost:7420"), StatusCode::OK),
        (None, StatusCode::OK),
    ] {
        let mut request = read_file().header(header::AUTHORIZATION, &bearer);
        if let Some(origin) = origin {
            request = request.header(header::ORIGIN, origin);
        }
        assert_eq!(send(&token, request.body(Body::empty()).unwrap()).await.status, expected, "{origin:?}");
    }
}

#[tokio::test]
async fn every_response_carries_the_security_headers_and_no_cors() {
    let token = token();
    let preflight = Request::builder()
        .method(Method::OPTIONS)
        .uri(READ_FILE)
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "http://evil.example")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST");
    let sign_in = get(&format!("/?token={}", token.expose()));
    for request in [preflight, sign_in, get("/")] {
        let reply = send(&token, request.body(Body::empty()).unwrap()).await;
        assert_eq!(reply.headers[header::CACHE_CONTROL], "no-store");
        assert_eq!(reply.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(reply.headers[header::REFERRER_POLICY], "no-referrer");
        let csp = reply.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.starts_with("default-src 'self'; script-src 'self' 'sha256-"), "{csp}");
        assert!(!reply.headers.keys().any(|name| name.as_str().starts_with("access-control-")));
    }
}
