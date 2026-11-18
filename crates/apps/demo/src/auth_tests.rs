use axum::http::request::Builder;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};

use super::{CLIENT, TOKEN_WINDOW, Tokens};
use crate::router;
use crate::testing::{send, sent};

const FORM: &str = "application/x-www-form-urlencoded";

/// `request` with the `authorization` header, if any.
fn authorized(request: Builder, authorization: Option<&str>) -> Builder {
    match authorization {
        Some(value) => request.header(header::AUTHORIZATION, value),
        None => request,
    }
}

/// `GET /auth/me` with `authorization`, if any.
async fn me(app: &axum::Router, authorization: Option<&str>) -> (StatusCode, Value) {
    sent(app, authorized(Request::builder().uri("/auth/me"), authorization), "").await
}

/// `POST /auth/logout` with `authorization`, if any.
async fn logout(app: &axum::Router, authorization: Option<&str>) -> (StatusCode, Value) {
    let request = Request::builder().method(Method::POST).uri("/auth/logout");
    sent(app, authorized(request, authorization), "").await
}

/// What `/auth/me` and `/auth/logout` answer without a valid token.
fn refused() -> (StatusCode, Value) {
    (StatusCode::UNAUTHORIZED, json!({"error": "a valid bearer token is needed"}))
}

/// `POST /oauth/token` with `authorization`, if any, and `form`.
async fn grant(app: &axum::Router, authorization: Option<&str>, form: &str) -> (StatusCode, Value) {
    let request = Request::builder().method(Method::POST).uri("/oauth/token").header(header::CONTENT_TYPE, FORM);
    sent(app, authorized(request, authorization), form).await
}

#[tokio::test]
async fn a_login_token_opens_auth_me() {
    let app = router();
    let (status, issued) = send(&app, Method::POST, "/auth/token", r#"{"username": "ada"}"#).await;
    let expected = json!({"access_token": "demo-token-1", "token_type": "Bearer", "expires_in": 3600});
    assert_eq!((status, &issued), (StatusCode::OK, &expected));
    let bearer = format!("Bearer {}", issued["access_token"].as_str().unwrap());
    assert_eq!(me(&app, Some(&bearer)).await, (StatusCode::OK, json!({"user": "demo"})), "the token is not echoed");
    assert_eq!(me(&app, Some(&bearer)).await.0, StatusCode::OK, "and it can be used again");
    assert_eq!(send(&app, Method::POST, "/auth/token", "").await.1["access_token"], "demo-token-2");
}

#[tokio::test]
async fn auth_me_refuses_a_missing_or_unknown_token() {
    let app = router();
    send(&app, Method::POST, "/auth/token", "").await;
    let refused = refused();
    for authorization in [None, Some("Bearer demo-token-2"), Some("Bearer demo-token-01"), Some("bearer demo-token-1")]
    {
        assert_eq!(me(&app, authorization).await, refused, "{authorization:?}");
    }
    for authorization in ["Bearer demo-token-0", "Bearer token-1", "Basic demo-token-1", "Bearer "] {
        assert_eq!(me(&app, Some(authorization)).await, refused, "{authorization}");
    }
    let request = Request::builder().uri("/auth/me");
    let response = tower::ServiceExt::oneshot(app, request.body(axum::body::Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.headers().get(header::WWW_AUTHENTICATE).unwrap(), "Bearer");
}

#[tokio::test]
async fn only_the_newest_tokens_are_valid() {
    let tokens = Tokens::default();
    let first = tokens.issue();
    assert_eq!(tokens.valid(&first), Some(1));
    for _ in 1..TOKEN_WINDOW {
        tokens.issue();
    }
    assert_eq!(tokens.valid(&first), Some(1), "the oldest of the window");
    let newest = tokens.issue();
    assert_eq!(tokens.valid(&first), None, "pushed out of the window");
    assert!(tokens.valid("demo-token-2").is_some() && tokens.valid(&newest).is_some());
    assert_eq!(tokens.valid(&format!("demo-token-{}", TOKEN_WINDOW + 2)), None, "not handed out yet");
}

#[tokio::test]
async fn the_client_gets_a_token_for_client_credentials() {
    let app = router();
    let (status, issued) = grant(&app, Some(CLIENT), "grant_type=client_credentials&scope=users.read").await;
    assert_eq!(
        (status, &issued["token_type"], &issued["expires_in"]),
        (StatusCode::OK, &json!("Bearer"), &json!(3600))
    );
    let bearer = format!("Bearer {}", issued["access_token"].as_str().unwrap());
    assert_eq!(me(&app, Some(&bearer)).await.0, StatusCode::OK, "it opens /auth/me too");
    let login = send(&app, Method::POST, "/auth/token", "").await.1;
    assert_eq!(login["access_token"], "demo-token-2", "one counter for both");
}

#[tokio::test]
async fn the_token_url_refuses_other_clients_and_grants() {
    let app = router();
    let invalid_client = (StatusCode::UNAUTHORIZED, json!({"error": "invalid_client"}));
    let wrong_secret = "Basic dmMtY2xpZW50Om5vcGU=";
    for authorization in [None, Some(wrong_secret), Some("Bearer demo-token-1")] {
        assert_eq!(
            grant(&app, authorization, "grant_type=client_credentials").await,
            invalid_client,
            "{authorization:?}"
        );
    }
    let unsupported = (StatusCode::BAD_REQUEST, json!({"error": "unsupported_grant_type"}));
    for form in ["grant_type=password&username=a&password=b", "grant_type=", "scope=users.read"] {
        assert_eq!(grant(&app, Some(CLIENT), form).await, unsupported, "{form}");
    }
    let request = Request::builder().method(Method::POST).uri("/oauth/token").header(header::AUTHORIZATION, CLIENT);
    let not_a_form = sent(&app, request.header(header::CONTENT_TYPE, "application/json"), "{}").await;
    assert_eq!(not_a_form, (StatusCode::BAD_REQUEST, json!({"error": "invalid_request"})));
    assert_eq!(send(&app, Method::POST, "/auth/token", "").await.1["access_token"], "demo-token-1", "none handed out");
}

#[tokio::test]
async fn a_logout_revokes_only_its_token_and_a_new_login_works() {
    let app = router();
    send(&app, Method::POST, "/auth/token", "").await;
    send(&app, Method::POST, "/auth/token", "").await;
    assert_eq!(logout(&app, Some("Bearer demo-token-1")).await, (StatusCode::NO_CONTENT, Value::Null));
    assert_eq!(me(&app, Some("Bearer demo-token-1")).await, refused(), "revoked");
    assert_eq!(logout(&app, Some("Bearer demo-token-1")).await, refused(), "and it cannot log out again");
    assert_eq!(me(&app, Some("Bearer demo-token-2")).await.0, StatusCode::OK, "the other token is untouched");
    let fresh = send(&app, Method::POST, "/auth/token", "").await.1;
    assert_eq!(fresh["access_token"], "demo-token-3");
    assert_eq!(me(&app, Some("Bearer demo-token-3")).await.0, StatusCode::OK, "a new login works");
}

#[tokio::test]
async fn a_logout_refuses_a_missing_unknown_or_old_token() {
    let app = router();
    send(&app, Method::POST, "/auth/token", "").await;
    for authorization in [None, Some("Bearer demo-token-2"), Some("Bearer demo-token-01"), Some("Basic demo-token-1")] {
        assert_eq!(logout(&app, authorization).await, refused(), "{authorization:?}");
    }
    for _ in 0..TOKEN_WINDOW {
        send(&app, Method::POST, "/auth/token", "").await;
    }
    assert_eq!(logout(&app, Some("Bearer demo-token-1")).await, refused(), "out of the window");
    assert_eq!(me(&app, Some("Bearer demo-token-2")).await.0, StatusCode::OK, "no refusal revoked anything");
}

#[test]
fn a_revoke_never_reaches_a_newer_token_in_the_same_slot() {
    let tokens = Tokens::default();
    tokens.issue();
    tokens.revoke(1);
    for _ in 0..TOKEN_WINDOW {
        tokens.issue();
    }
    let reused = format!("demo-token-{}", TOKEN_WINDOW + 1);
    assert_eq!(tokens.valid(&reused), Some(TOKEN_WINDOW + 1), "the slot of a revoked token, reused");
    tokens.revoke(TOKEN_WINDOW + 1);
    assert_eq!(tokens.valid(&reused), None);
    assert_eq!(tokens.valid("demo-token-2"), Some(2), "its neighbours are untouched");

    let late = Tokens::default();
    for _ in 0..=TOKEN_WINDOW {
        late.issue();
    }
    late.revoke(1);
    assert_eq!(late.valid(&reused), Some(TOKEN_WINDOW + 1), "an old token's revoke, arriving late");
    late.revoke(TOKEN_WINDOW + 1);
    late.revoke(1);
    assert_eq!(late.valid(&reused), None, "an older revoke does not undo a newer one");
}
