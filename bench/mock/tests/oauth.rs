//! `/oauth/token` and `/oauth/authorize`.

mod support;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, LOCATION};
use reqwest::{Client, Method, StatusCode, redirect};
use serde_json::Value;
use support::{Mock, TestResult};

const CLIENT: &str = "Basic dmMtY2xpZW50OnZjLXNlY3JldA==";

async fn token(mock: &Mock, client: Option<&str>, form: &str) -> Result<(StatusCode, Value), reqwest::Error> {
    let mut request =
        mock.request(Method::POST, "/oauth/token").header(CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(client) = client {
        request = request.header(AUTHORIZATION, client);
    }
    let response = request.body(form.to_owned()).send().await?;
    Ok((response.status(), response.json().await?))
}

#[tokio::test]
async fn each_grant_issues_a_token_that_auth_me_accepts() -> TestResult {
    let mock = Mock::start().await?;
    let (status, issued) = token(&mock, Some(CLIENT), "grant_type=client_credentials").await?;
    assert_eq!((status, issued.get("refresh_token")), (StatusCode::OK, None));
    let access = issued["access_token"].as_str().ok_or("no access_token")?;
    assert_eq!(mock.get("/auth/me").bearer_auth(access).send().await?.status(), StatusCode::OK);
    let (_, signed_in) =
        token(&mock, None, "grant_type=password&username=ada&password=lovelace&client_id=vc-public").await?;
    let refresh = signed_in["refresh_token"].as_str().ok_or("no refresh_token")?;
    let (status, refreshed) =
        token(&mock, Some(CLIENT), &format!("grant_type=refresh_token&refresh_token={refresh}")).await?;
    assert_eq!((status, refreshed["expires_in"].as_u64()), (StatusCode::OK, Some(3600)));
    let (again, _) = token(&mock, Some(CLIENT), &format!("grant_type=refresh_token&refresh_token={refresh}")).await?;
    assert_eq!(again, StatusCode::BAD_REQUEST, "a refresh token is used once");
    assert_eq!(mock.handle.stats().await?.logins, 3);
    Ok(())
}

#[tokio::test]
async fn expires_in_on_the_token_url_sets_the_lifetime() -> TestResult {
    let mock = Mock::start().await?;
    let request = mock.request(Method::POST, "/oauth/token?expiresIn=30").header(AUTHORIZATION, CLIENT);
    let response =
        request.header(CONTENT_TYPE, "application/x-www-form-urlencoded").body("grant_type=client_credentials");
    let issued: Value = response.send().await?.json().await?;
    assert_eq!(issued["expires_in"].as_u64(), Some(30));
    Ok(())
}

#[tokio::test]
async fn unknown_clients_wrong_passwords_and_unknown_grants_are_refused() -> TestResult {
    let mock = Mock::start().await?;
    let (status, body) = token(&mock, Some("Basic eDp5"), "grant_type=client_credentials").await?;
    assert_eq!((status, body["error"].as_str()), (StatusCode::UNAUTHORIZED, Some("invalid_client")));
    let (status, body) = token(&mock, Some(CLIENT), "grant_type=password&username=ada&password=nope").await?;
    assert_eq!((status, body["error"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_grant")));
    let (status, _) = token(&mock, Some(CLIENT), "grant_type=implicit").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn authorize_redirects_with_a_code_that_is_exchanged_once() -> TestResult {
    let mock = Mock::start().await?;
    let client = Client::builder().redirect(redirect::Policy::none()).build()?;
    let url = format!("{}/oauth/authorize?redirect_uri=http://127.0.0.1:9/cb&state=xyz", mock.handle.base_url());
    let response = client.get(url).send().await?;
    assert!(response.status().is_redirection());
    let location = response.headers().get(LOCATION).ok_or("no location")?.to_str()?.to_owned();
    assert_eq!(location, "http://127.0.0.1:9/cb?code=mock-code-1&state=xyz");
    let exchange = "grant_type=authorization_code&code=mock-code-1&client_id=vc-public";
    let (status, issued) = token(&mock, None, exchange).await?;
    assert!(status == StatusCode::OK && issued["refresh_token"].is_string());
    assert_eq!(token(&mock, None, exchange).await?.0, StatusCode::BAD_REQUEST);
    let missing = client.get(format!("{}/oauth/authorize", mock.handle.base_url())).send().await?;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    Ok(())
}
