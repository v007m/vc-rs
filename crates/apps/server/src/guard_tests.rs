use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::routing::post;
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
    let mcp = Router::new().route("/", post(|| async { "mcp" }));
    let builder = ServerBuilder::new(0).service(service).mcp(mcp).files(Arc::new(TestFiles));
    let app = builder.guarded(Arc::clone(token), 7420);
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

fn mcp() -> axum::http::request::Builder {
    Request::builder().method(Method::POST).uri("/mcp").header(header::HOST, HOST)
}

fn token() -> Arc<Token> {
    Arc::new(Token::generate().unwrap())
}

fn cookie(token: &Token) -> String {
    format!("vc_token_7420={}", token.session())
}

fn bearer(secret: &str) -> String {
    format!("Bearer {secret}")
}

#[tokio::test]
async fn the_launch_url_sets_the_cookie_once_and_redirects_without_the_token() {
    let token = token();
    let launch = format!("/?token={}", token.expose());
    let reply = send(&token, get(&launch).body(Body::empty()).unwrap()).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER);
    assert_eq!(reply.headers[header::LOCATION], "/");
    let set = reply.headers[header::SET_COOKIE].to_str().unwrap();
    assert_eq!(set, format!("{}; Path=/; HttpOnly; SameSite=Strict", cookie(&token)));
    assert!(reply.headers[header::SET_COOKIE].is_sensitive());
    let again = send(&token, get(&launch).body(Body::empty()).unwrap()).await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED);
    assert!(!again.headers.contains_key(header::SET_COOKIE));
    let said = String::from_utf8_lossy(&again.body);
    assert!(said.contains("use the open tab or restart vc ui") && !said.contains(token.expose()), "{said}");
}

#[tokio::test]
async fn a_wrong_launch_token_signs_nobody_in_is_not_echoed_and_uses_nothing_up() {
    let token = token();
    let reply = send(&token, get("/?token=not-the-token-0123456789").body(Body::empty()).unwrap()).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(!reply.headers.contains_key(header::SET_COOKIE));
    assert!(!String::from_utf8_lossy(&reply.body).contains("not-the-token"));
    let launch = get(&format!("/?token={}", token.expose()));
    assert_eq!(send(&token, launch.body(Body::empty()).unwrap()).await.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn the_token_in_any_other_url_is_refused() {
    let token = token();
    for (uri, expected) in [
        (format!("/runs?token={}", token.expose()), StatusCode::BAD_REQUEST),
        (format!("/mcp?token={}", token.agent()), StatusCode::BAD_REQUEST),
        (format!("/?a=1&token={}", token.expose()), StatusCode::SEE_OTHER),
    ] {
        assert_eq!(send(&token, get(&uri).body(Body::empty()).unwrap()).await.status, expected, "{uri}");
    }
    let post = read_file().uri(format!("{READ_FILE}?token={}", token.expose())).header(header::COOKIE, cookie(&token));
    assert_eq!(send(&token, post.body(Body::empty()).unwrap()).await.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_cookie_lets_the_browser_in_and_the_agent_token_lets_mcp_in() {
    let token = token();
    let cookies = format!("theme=dark; {}", cookie(&token));
    let page = send(&token, get("/runs/12").header(header::COOKIE, cookies).body(Body::empty()).unwrap()).await;
    assert_eq!((page.status, page.body.as_ref()), (StatusCode::OK, INDEX_HTML.as_bytes()));
    let agent = mcp().header(header::AUTHORIZATION, format!("bearer {}", token.agent()));
    let reply = send(&token, agent.body(Body::empty()).unwrap()).await;
    assert_eq!((reply.status, reply.body.as_ref()), (StatusCode::OK, &b"mcp"[..]));
}

#[tokio::test]
async fn a_request_without_its_secret_is_unauthorized() {
    let token = token();
    let requests = [
        get("/"),
        get("/").header(header::COOKIE, "vc_token_7420=guess"),
        get("/").header(header::COOKIE, format!("vc_token={}", token.session())),
        get("/").header(header::COOKIE, format!("vc_token_7421={}", token.session())),
        mcp(),
        mcp().header(header::AUTHORIZATION, "Bearer guess"),
        mcp().header(header::AUTHORIZATION, token.agent()),
        mcp().uri("/mcp/sessions").header(header::AUTHORIZATION, bearer(token.expose())),
    ];
    for request in requests {
        assert_eq!(send(&token, request.body(Body::empty()).unwrap()).await.status, StatusCode::UNAUTHORIZED);
    }
}

#[tokio::test]
async fn each_secret_opens_only_its_own_door() {
    let token = token();
    let ours = "http://127.0.0.1:7420";
    let requests = [
        get("/").header(header::COOKIE, format!("vc_token_7420={}", token.expose())),
        get("/").header(header::COOKIE, format!("vc_token_7420={}", token.agent())),
        read_file().header(header::AUTHORIZATION, bearer(token.agent())),
        read_file().header(header::AUTHORIZATION, bearer(token.expose())),
        read_file().header(header::AUTHORIZATION, bearer(token.agent())).header(header::COOKIE, cookie(&token)),
        mcp().header(header::AUTHORIZATION, bearer(token.session())),
        mcp().header(header::AUTHORIZATION, bearer(token.expose())),
        mcp().header(header::COOKIE, cookie(&token)).header(header::ORIGIN, ours),
    ];
    for request in requests {
        assert_eq!(send(&token, request.body(Body::empty()).unwrap()).await.status, StatusCode::UNAUTHORIZED);
    }
    assert!(token.agent() != token.session() && token.agent() != token.expose());
}

#[tokio::test]
async fn a_write_the_cookie_signs_in_must_carry_our_origin() {
    let token = token();
    let without = read_file().header(header::COOKIE, cookie(&token));
    assert_eq!(send(&token, without.body(Body::empty()).unwrap()).await.status, StatusCode::FORBIDDEN);
    let ours = read_file().header(header::COOKIE, cookie(&token)).header(header::ORIGIN, "http://127.0.0.1:7420");
    assert_eq!(send(&token, ours.body(Body::empty()).unwrap()).await.status, StatusCode::OK);
}

#[tokio::test]
async fn only_this_servers_host_names_are_answered() {
    let token = token();
    for (host, expected) in [
        ("localhost:7420", StatusCode::OK),
        ("LOCALHOST:7420", StatusCode::OK),
        ("evil.example:7420", StatusCode::FORBIDDEN),
        ("127.0.0.1:7421", StatusCode::FORBIDDEN),
        ("127.0.0.1", StatusCode::FORBIDDEN),
    ] {
        let mut request = get("/").header(header::COOKIE, cookie(&token)).body(Body::empty()).unwrap();
        request.headers_mut().insert(header::HOST, host.parse().unwrap());
        assert_eq!(send(&token, request).await.status, expected, "{host}");
    }
}

#[tokio::test]
async fn a_write_from_another_sites_page_is_forbidden() {
    let token = token();
    for (origin, expected) in [
        (Some("http://evil.example"), StatusCode::FORBIDDEN),
        (Some("http://127.0.0.1:7421"), StatusCode::FORBIDDEN),
        (Some("null"), StatusCode::FORBIDDEN),
        (Some("http://localhost:7420"), StatusCode::OK),
        (None, StatusCode::OK),
    ] {
        let mut request = mcp().header(header::AUTHORIZATION, bearer(token.agent()));
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
