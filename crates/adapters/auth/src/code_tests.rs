use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::digest::{SHA256, digest};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::mpsc::UnboundedReceiver;
use vc_interfaces::{AuthError, AuthProvider, Keychain, Secret};
use vc_testkit::{MemoryKeychain, RecordingBrowser};

use super::AuthorizationCode;
use crate::testing::{TOKEN, field, form, request, token_server};

fn settings(extra: &Value) -> Value {
    let mut settings = json!({
        "authorizeUrl": "https://id.test/authorize",
        "tokenUrl": "https://id.test/token",
        "clientId": "cli",
        "scopes": ["openid", "profile"],
    });
    settings.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
    settings
}

/// The query of `url`, decoded.
fn query(url: &str) -> Vec<(String, String)> {
    let query = url.split_once('?').map(|(_, query)| query).unwrap_or_default();
    let decode =
        |text: &str| percent_encoding::percent_decode_str(&text.replace('+', " ")).decode_utf8_lossy().into_owned();
    query.split('&').filter_map(|pair| pair.split_once('=')).map(|(key, value)| (decode(key), decode(value))).collect()
}

/// Sends `GET <target>` to the redirect listener and gives the status line.
async fn get(port: &str, target: &str) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).await.unwrap();
    stream.write_all(format!("GET {target} HTTP/1.1\r\nhost: localhost\r\n\r\n").as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    answer.lines().next().unwrap_or_default().to_owned()
}

/// The redirect listener's port, from the authorize URL's query.
fn port(pairs: &[(String, String)]) -> String {
    let redirect = field(pairs, "redirect_uri").unwrap();
    redirect.strip_prefix("http://127.0.0.1:").unwrap().strip_suffix("/callback").unwrap().to_owned()
}

/// Plays the browser: checks the authorize URL, then follows the redirect with `state` (the
/// URL's own when `None`). Gives the authorize URL's query.
async fn sign_in(mut urls: UnboundedReceiver<String>, state: Option<&str>) -> Vec<(String, String)> {
    let url = urls.recv().await.unwrap();
    assert!(url.starts_with("https://id.test/authorize?"), "{url}");
    let pairs = query(&url);
    let port = port(&pairs);
    assert_eq!(get(&port, "/favicon.ico").await, "HTTP/1.1 404 Not Found");
    let state = state.unwrap_or_else(|| field(&pairs, "state").unwrap());
    let line = get(&port, &format!("/callback?code=c0de&state={state}")).await;
    let expected = if field(&pairs, "state") == Some(state) { "HTTP/1.1 200 OK" } else { "HTTP/1.1 400 Bad Request" };
    assert_eq!(line, expected);
    pairs
}

#[tokio::test]
async fn a_sign_in_exchanges_the_code_with_its_verifier_and_keeps_the_refresh_token() {
    let (transport, recordings) = token_server(TOKEN);
    let (browser, urls) = RecordingBrowser::new();
    let keychain = Arc::new(MemoryKeychain::new());
    let code = AuthorizationCode::new(transport, Arc::new(browser), Some(Arc::clone(&keychain) as _));
    let auth = request("oauth2AuthorizationCode", &settings(&json!({})));
    let (got, pairs) = tokio::join!(code.credentials(&auth), sign_in(urls, None));
    assert_eq!(got.unwrap().headers[0].1.expose(), "Bearer t0k3n-abc");
    assert_eq!(field(&pairs, "response_type"), Some("code"));
    assert_eq!(field(&pairs, "client_id"), Some("cli"));
    assert_eq!(field(&pairs, "scope"), Some("openid profile"));
    assert_eq!(field(&pairs, "code_challenge_method"), Some("S256"));
    let sent = form(&recordings.take().remove(0).request);
    assert_eq!(field(&sent, "grant_type"), Some("authorization_code"));
    assert_eq!(field(&sent, "code"), Some("c0de"));
    assert_eq!(field(&sent, "redirect_uri"), field(&pairs, "redirect_uri"));
    let verifier = field(&sent, "code_verifier").unwrap();
    assert_eq!(verifier.len(), 43, "32 random bytes");
    let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
    assert_eq!(field(&pairs, "code_challenge"), Some(challenge.as_str()));
    assert_eq!(field(&pairs, "state").map(str::len), Some(22), "16 random bytes");
    let kept = keychain.get("vibranium", "api@cli").unwrap();
    assert_eq!(kept.as_ref().map(Secret::expose), Some("r3fresh-1"));
}

#[tokio::test]
async fn a_kept_refresh_token_skips_the_browser() {
    let (transport, recordings) = token_server(TOKEN);
    let (browser, mut urls) = RecordingBrowser::new();
    let keychain = Arc::new(MemoryKeychain::new());
    keychain.set("vibranium", "api@cli", &Secret::new("kept")).unwrap();
    let code = AuthorizationCode::new(transport, Arc::new(browser), Some(Arc::clone(&keychain) as _));
    let got = code.credentials(&request("oauth2AuthorizationCode", &settings(&json!({})))).await;
    assert_eq!(got.unwrap().headers[0].1.expose(), "Bearer t0k3n-abc");
    let sent = form(&recordings.take().remove(0).request);
    assert_eq!(field(&sent, "grant_type"), Some("refresh_token"));
    assert_eq!(field(&sent, "refresh_token"), Some("kept"));
    assert!(urls.try_recv().is_err(), "no browser opened");
    assert_eq!(keychain.get("vibranium", "api@cli").unwrap().as_ref().map(Secret::expose), Some("r3fresh-1"));
}

#[tokio::test]
async fn a_refused_refresh_token_is_forgotten() {
    let (transport, _) = token_server(TOKEN);
    let (browser, _urls) = RecordingBrowser::new();
    let keychain = Arc::new(MemoryKeychain::new());
    keychain.set("vibranium", "api@cli", &Secret::new("stale")).unwrap();
    let code = AuthorizationCode::new(transport, Arc::new(browser), Some(Arc::clone(&keychain) as _));
    let refusing = settings(&json!({"tokenUrl": "https://id.test/refuse", "authorizeUrl": null}));
    let got = code.credentials(&request("oauth2AuthorizationCode", &refusing)).await;
    assert_eq!(got.err(), Some(AuthError::Missing { auth: "api".into(), key: "authorizeUrl".into() }));
    assert_eq!(keychain.get("vibranium", "api@cli").unwrap(), None);
}

#[tokio::test]
async fn a_refresh_that_fails_for_a_while_keeps_the_token_and_gives_the_error() {
    let (transport, _) = token_server(TOKEN);
    let (browser, mut urls) = RecordingBrowser::new();
    let keychain = Arc::new(MemoryKeychain::new());
    keychain.set("vibranium", "api@cli", &Secret::new("kept")).unwrap();
    let code = AuthorizationCode::new(transport, Arc::new(browser), Some(Arc::clone(&keychain) as _));
    let busy = settings(&json!({"tokenUrl": "https://id.test/busy"}));
    let got = code.credentials(&request("oauth2AuthorizationCode", &busy)).await;
    assert_eq!(got.err(), Some(AuthError::Rejected { auth: "api".into(), status: 503 }));
    assert_eq!(keychain.get("vibranium", "api@cli").unwrap().as_ref().map(Secret::expose), Some("kept"));
    assert!(urls.try_recv().is_err(), "no browser opened");
}

#[tokio::test]
async fn a_redirect_with_another_state_is_refused_and_the_sign_in_waits_for_its_own() {
    let (transport, recordings) = token_server(TOKEN);
    let (browser, urls) = RecordingBrowser::new();
    let code = AuthorizationCode::new(transport, Arc::new(browser), None);
    let auth = request("oauth2AuthorizationCode", &settings(&json!({})));
    let browse = async {
        let pairs = sign_in(urls, Some("forged")).await;
        let own = format!("/callback?code=r3al&state={}", field(&pairs, "state").unwrap());
        get(&port(&pairs), &own).await
    };
    let (got, line) = tokio::join!(code.credentials(&auth), browse);
    assert_eq!(line, "HTTP/1.1 200 OK");
    assert!(got.is_ok(), "{got:?}");
    let mut sent = recordings.take();
    assert_eq!(sent.len(), 1, "only the sign-in's own code was exchanged");
    assert_eq!(field(&form(&sent.remove(0).request), "code"), Some("r3al"));
}

#[tokio::test]
async fn an_idle_connection_does_not_hold_up_the_redirect() {
    let (transport, _) = token_server(TOKEN);
    let (browser, mut urls) = RecordingBrowser::new();
    let code = AuthorizationCode::new(transport, Arc::new(browser), None);
    let auth = request("oauth2AuthorizationCode", &settings(&json!({})));
    let browse = async {
        let pairs = query(&urls.recv().await.unwrap());
        let port = port(&pairs);
        let _preconnect = TcpStream::connect(format!("127.0.0.1:{port}")).await.unwrap();
        get(&port, &format!("/callback?code=c0de&state={}", field(&pairs, "state").unwrap())).await
    };
    let both = async { tokio::join!(code.credentials(&auth), browse) };
    let (got, line) = tokio::time::timeout(Duration::from_secs(30), both).await.expect("the redirect was read");
    assert_eq!(line, "HTTP/1.1 200 OK");
    assert!(got.is_ok(), "{got:?}");
}

#[tokio::test]
async fn without_pkce_neither_challenge_nor_verifier_is_sent() {
    let (transport, recordings) = token_server(TOKEN);
    let (browser, urls) = RecordingBrowser::new();
    let code = AuthorizationCode::new(transport, Arc::new(browser), None);
    let extra = json!({"pkce": false, "authorizeUrl": "https://id.test/authorize?tenant=t", "scopes": null});
    let auth = request("oauth2AuthorizationCode", &settings(&extra));
    let (got, pairs) = tokio::join!(code.credentials(&auth), sign_in(urls, None));
    assert!(got.is_ok());
    assert_eq!(field(&pairs, "tenant"), Some("t"));
    assert_eq!(field(&pairs, "code_challenge"), None);
    assert_eq!(field(&pairs, "scope"), None);
    assert_eq!(field(&form(&recordings.take().remove(0).request), "code_verifier"), None);
}

#[tokio::test]
async fn a_bad_redirect_port_or_a_browser_that_will_not_open_fails_the_sign_in() {
    let (transport, _) = token_server(TOKEN);
    let (browser, urls) = RecordingBrowser::new();
    let code = AuthorizationCode::new(transport, Arc::new(browser), None);
    let bad_port = request("oauth2AuthorizationCode", &settings(&json!({"redirectPort": 70000})));
    assert!(
        matches!(code.credentials(&bad_port).await, Err(AuthError::Browser { ref reason, .. }) if reason.contains("port"))
    );
    drop(urls);
    let closed = code.credentials(&request("oauth2AuthorizationCode", &settings(&json!({})))).await;
    assert!(matches!(closed, Err(AuthError::Browser { .. })), "{closed:?}");
    let no_client = request("oauth2AuthorizationCode", &settings(&json!({"clientId": null})));
    assert_eq!(
        code.credentials(&no_client).await.err(),
        Some(AuthError::Missing { auth: "api".into(), key: "clientId".into() })
    );
}
