//! `/auth/token`, `/auth/me` and `/echo`.

mod support;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use support::{Mock, TestResult};

#[tokio::test]
async fn logins_are_counted_and_only_issued_tokens_are_accepted() -> TestResult {
    let mock = Mock::start().await?;
    let token: Value = mock.request(Method::POST, "/auth/token").send().await?.json().await?;
    assert_eq!(token["token_type"], "Bearer");
    assert_eq!(token["expires_in"], 3600);
    let access = token["access_token"].as_str().ok_or("no access_token")?;
    mock.request(Method::POST, "/auth/token").send().await?;

    let me = mock.get("/auth/me").bearer_auth(access).send().await?;
    assert_eq!(me.status(), StatusCode::OK);
    assert_eq!(mock.get("/auth/me").send().await?.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(mock.get("/auth/me").bearer_auth("forged").send().await?.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(mock.handle.stats().await?.logins, 2);
    Ok(())
}

#[tokio::test]
async fn echo_returns_the_request() -> TestResult {
    let mock = Mock::start().await?;
    let request = mock.request(Method::PUT, "/echo?a=1&b=two").header("x-trace", "t1").json(&json!({ "k": [1, 2] }));
    let echoed: Value = request.send().await?.json().await?;
    assert_eq!(echoed["method"], "PUT");
    assert_eq!(echoed["path"], "/echo");
    assert_eq!(echoed["query"], json!({ "a": "1", "b": "two" }));
    assert_eq!(echoed["headers"]["x-trace"], "t1");
    assert_eq!(echoed["body"], json!({ "k": [1, 2] }));
    assert_eq!(mock.handle.stats().await?.count("PUT /echo"), 1);
    Ok(())
}

#[tokio::test]
async fn echo_returns_text_bodies_as_strings_and_no_body_as_null() -> TestResult {
    let mock = Mock::start().await?;
    let text: Value = mock.request(Method::POST, "/echo").body("plain").send().await?.json().await?;
    assert_eq!(text["body"], "plain");
    let empty: Value = mock.get("/echo").send().await?.json().await?;
    assert_eq!(empty["body"], Value::Null);
    Ok(())
}
