use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use vc_interfaces::{AuthError, TransportError};
use vc_testkit::{FakeTransport, status};

use super::{TokenEndpoint, encode, scopes};
use crate::testing::{TOKEN, answering, field, form, request, token_server};

const GRANT: [(&str, &str); 1] = [("grant_type", "client_credentials")];

#[test]
fn encode_is_the_whatwg_form_encoding_a_form_body_uses() {
    assert_eq!(encode(&[("a b", "x&y=z"), ("k", "A-z0_9.*~")]), "a+b=x%26y%3Dz&k=A-z0_9.*%7E");
    assert_eq!(encode(&[]), "");
}

#[test]
fn scopes_join_a_list_and_keep_a_string() {
    assert_eq!(scopes(&request("t", &json!({"scopes": ["a", "b", 3]}))), Some("a b".to_owned()));
    assert_eq!(scopes(&request("t", &json!({"scopes": "read write"}))), Some("read write".to_owned()));
    assert_eq!(scopes(&request("t", &json!({"scopes": 3}))), None);
    assert_eq!(scopes(&request("t", &json!({}))), None);
}

#[tokio::test]
async fn a_confidential_client_authenticates_with_basic() {
    let (transport, recordings) = token_server(TOKEN);
    let settings =
        json!({"tokenUrl": "https://id.test/token", "clientId": "c:id", "clientSecret": "s+cret", "audience": "aud"});
    let token = TokenEndpoint::new(transport).token(&request("t", &settings), &GRANT).await.unwrap();
    let sent = recordings.take().remove(0).request;
    let basic = format!("Basic {}", STANDARD.encode("c%3Aid:s%2Bcret"));
    assert_eq!(sent.headers.get("authorization").unwrap(), basic.as_str());
    assert_eq!(sent.headers.get("content-type").unwrap(), "application/x-www-form-urlencoded");
    assert_eq!(sent.headers.get("accept").unwrap(), "application/json");
    assert_eq!(sent.method, http::Method::POST);
    let fields = form(&sent);
    assert_eq!(field(&fields, "grant_type"), Some("client_credentials"));
    assert_eq!(field(&fields, "audience"), Some("aud"));
    assert_eq!(field(&fields, "client_id"), None);
    assert_eq!(token.refresh.as_deref(), Some("r3fresh-1"));
    assert_eq!(token.credentials.headers[0].1.expose(), "Bearer t0k3n-abc");
    assert_eq!(token.credentials.expires_in, Some(Duration::from_secs(60)));
}

#[tokio::test]
async fn a_public_client_sends_its_id_in_the_form() {
    let (transport, recordings) = token_server(r#"{"access_token":"x","token_type":"MAC","expires_in":"90"}"#);
    let settings = json!({"tokenUrl": "https://id.test/token", "clientId": "cli"});
    let token = TokenEndpoint::new(transport).token(&request("t", &settings), &GRANT).await.unwrap();
    let sent = recordings.take().remove(0).request;
    assert!(sent.headers.get("authorization").is_none());
    assert_eq!(field(&form(&sent), "client_id"), Some("cli"));
    assert_eq!(field(&form(&sent), "audience"), None);
    assert_eq!(token.credentials.headers[0].1.expose(), "MAC x");
    assert_eq!(token.credentials.expires_in, Some(Duration::from_secs(90)));
    assert_eq!(token.refresh, None);
}

/// The token request's body is the WHATWG form encoding a form body uses (D273): a space is `+`.
#[tokio::test]
async fn a_token_request_is_form_encoded_with_a_space_as_plus() {
    let (transport, recordings) = token_server(TOKEN);
    let settings = json!({"tokenUrl": "https://id.test/token", "clientId": "my cli~", "audience": "a b"});
    let grant = [("grant_type", "client_credentials"), ("scope", "read write")];
    TokenEndpoint::new(transport).token(&request("t", &settings), &grant).await.unwrap();
    let sent = recordings.take().remove(0).request;
    let body = String::from_utf8_lossy(&sent.body).into_owned();
    assert_eq!(body, "grant_type=client_credentials&scope=read+write&audience=a+b&client_id=my+cli%7E");
}

#[tokio::test]
async fn failures_name_the_cause_without_the_body() {
    let settings = json!({"tokenUrl": "https://id.test/token", "clientId": "cli"});
    let auth = request("t", &settings);
    let malformed = |missing: &str| AuthError::Malformed { auth: "api".into(), missing: missing.into() };
    for (body, expected) in
        [("not json secret-body", malformed("JSON body")), (r#"{"token":"x"}"#, malformed("access_token"))]
    {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(answering(200, body))]));
        assert_eq!(TokenEndpoint::new(transport).token(&auth, &GRANT).await.err(), Some(expected));
    }
    let transport = Arc::new(FakeTransport::scripted(vec![Ok(answering(500, "secret-body"))]));
    let refused = TokenEndpoint::new(transport).token(&auth, &GRANT).await.err();
    assert_eq!(refused, Some(AuthError::Rejected { auth: "api".into(), status: 500 }));
    let down = Arc::new(FakeTransport::new(|_, _| Err(TransportError::Protocol { reason: "reset".into() })));
    let unreachable = TokenEndpoint::new(down).token(&auth, &GRANT).await.err().unwrap();
    assert!(matches!(unreachable, AuthError::Unreachable { ref reason, .. } if reason.contains("reset")));
}

#[tokio::test]
async fn the_token_url_and_client_id_are_required() {
    let transport = Arc::new(FakeTransport::scripted(vec![Ok(status(200))]));
    let endpoint = TokenEndpoint::new(transport);
    let missing = |key: &str| Some(AuthError::Missing { auth: "api".into(), key: key.into() });
    let bad_url = json!({"tokenUrl": "not a url", "clientId": "cli"});
    assert_eq!(endpoint.token(&request("t", &bad_url), &GRANT).await.err(), missing("tokenUrl"));
    assert_eq!(endpoint.token(&request("t", &json!({"clientId": "cli"})), &GRANT).await.err(), missing("tokenUrl"));
    let no_client = json!({"tokenUrl": "https://id.test/token"});
    assert_eq!(endpoint.token(&request("t", &no_client), &GRANT).await.err(), missing("clientId"));
}

#[tokio::test]
async fn the_largest_expires_in_as_a_number_or_a_string_is_passed_on_for_the_cache_to_cap() {
    const BODIES: [&str; 2] = [
        r#"{"access_token":"x","expires_in":18446744073709551615}"#,
        r#"{"access_token":"x","expires_in":"18446744073709551615"}"#,
    ];
    for body in BODIES {
        let (transport, _) = token_server(body);
        let settings = json!({"tokenUrl": "https://id.test/token", "clientId": "web"});
        let token = TokenEndpoint::new(transport).token(&request("t", &settings), &GRANT).await.unwrap();
        assert_eq!(token.credentials.expires_in, Some(Duration::from_secs(u64::MAX)), "{body}");
    }
}
