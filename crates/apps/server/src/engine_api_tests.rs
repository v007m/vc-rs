use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header, request};
use axum::routing::post;
use http_body_util::BodyExt as _;
use prost::Message as _;
use tower::ServiceExt as _;
use vc_contracts::engine::v1 as wire;
use vc_engine::EngineError;

use super::{EngineApi, SERVICE, failed};
use crate::engine_fixture::project;
use crate::token::Token;
use crate::{Code, ServerBuilder};

const HOST: &str = "127.0.0.1:7420";

/// The engine service and an MCP mount behind the guard, as `vc ui` serves them; with `mcp`
/// false, the engine service alone.
fn served(api: &EngineApi, token: &Arc<Token>, mcp: bool) -> Router {
    let engine = EngineApi::new(Arc::clone(&api.engine), Vec::new(), Vec::new()).into_service();
    let builder = ServerBuilder::new(0).service(engine);
    let builder = if mcp { builder.mcp(Router::new().route("/", post(|| async { "mcp" }))) } else { builder };
    builder.guarded(Arc::clone(token), 7420)
}

/// A browser page's call of `method`, with no secret yet.
fn rpc(method: &str) -> request::Builder {
    Request::builder()
        .method(Method::POST)
        .uri(format!("/api/{SERVICE}/{method}"))
        .header(header::HOST, HOST)
        .header(header::ORIGIN, "http://127.0.0.1:7420")
        .header(header::CONTENT_TYPE, "application/proto")
}

fn cookie(token: &Token) -> String {
    format!("vc_token_7420={}", token.session())
}

async fn status(app: &Router, request: request::Builder) -> StatusCode {
    app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap().status()
}

#[test]
fn an_engine_error_keeps_its_message_and_help_under_the_code_that_fits() {
    let stale = failed(&EngineError::Stale { reason: "a.test.yaml: nothing was written".into() });
    assert_eq!(stale.code, Code::Aborted);
    let refused =
        failed(&EngineError::Edit { reason: "`.env` is not a definition file".into(), help: Some("open one".into()) });
    assert_eq!(refused.code, Code::InvalidArgument);
    assert!(refused.message.ends_with("\nhelp: open one"), "{refused:?}");
    let missing = failed(&EngineError::NoFilesMatch { path: "a.test.yaml".into() });
    assert_eq!(missing.code, Code::NotFound);
}

#[test]
fn a_call_with_no_profile_takes_the_servers_and_its_sets_come_last() {
    let (_dir, api) = project();
    let engine = Arc::clone(&api.engine);
    let api = EngineApi::new(engine, vec!["bare".into()], vec!["vars.a=1".into()]);
    let (profiles, sets) = api.layers(Vec::new(), vec!["vars.a=2".into()]);
    assert_eq!((profiles, sets), (vec!["bare".to_owned()], vec!["vars.a=1".to_owned(), "vars.a=2".to_owned()]));
    let (profiles, _) = api.layers(vec!["dev".into()], Vec::new());
    assert_eq!(profiles, ["dev"]);
}

#[tokio::test]
async fn the_agent_token_opens_no_engine_rpc_and_the_cookie_does_not_open_mcp() {
    let (_dir, api) = project();
    let token = Arc::new(Token::generate().unwrap());
    let app = served(&api, &token, true);
    let agent = format!("Bearer {}", token.agent());
    for method in ["DecideApproval", "WriteFile", "Import", "Run", "Connection"] {
        let alone = rpc(method).header(header::AUTHORIZATION, &agent);
        assert_eq!(status(&app, alone).await, StatusCode::UNAUTHORIZED, "{method}");
        let both = rpc(method).header(header::AUTHORIZATION, &agent).header(header::COOKIE, cookie(&token));
        assert_eq!(status(&app, both).await, StatusCode::UNAUTHORIZED, "{method}: a bearer token is refused on /api");
    }
    let browser = rpc("DecideApproval").uri("/mcp").header(header::COOKIE, cookie(&token));
    assert_eq!(status(&app, browser).await, StatusCode::UNAUTHORIZED);
    assert_eq!(status(&app, rpc("GetProject").header(header::COOKIE, cookie(&token))).await, StatusCode::OK);
    let agent_in = rpc("DecideApproval").uri("/mcp").header(header::AUTHORIZATION, &agent);
    assert_eq!(status(&app, agent_in).await, StatusCode::OK);
}

#[tokio::test]
async fn connection_gives_the_browser_the_mcp_url_and_the_agent_token() {
    let (_dir, api) = project();
    let token = Arc::new(Token::generate().unwrap());
    let request = rpc("Connection").header(header::COOKIE, cookie(&token)).body(Body::empty()).unwrap();
    let response = served(&api, &token, true).oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let reply = wire::ConnectionResponse::decode(response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!((reply.mcp_url.as_str(), reply.agent_token.as_str()), ("http://127.0.0.1:7420/mcp", token.agent()));
    let alone = served(&api, &token, false);
    assert_eq!(status(&alone, rpc("Connection").header(header::COOKIE, cookie(&token))).await, StatusCode::NOT_FOUND);
}
