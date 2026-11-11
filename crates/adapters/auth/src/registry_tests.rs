use std::sync::Arc;

use serde_json::{Value, json};
use vc_interfaces::{AuthError, AuthProvider, Credentials, Keychain, Secret};
use vc_testkit::{FixedFlow, MemoryKeychain, RecordingBrowser, auth_provider_suite};

use super::AuthTypes;
use crate::testing::{TOKEN, field, form, request, token_server};

fn types(keychain: Option<Arc<MemoryKeychain>>) -> AuthTypes {
    let (transport, _) = token_server(TOKEN);
    let (browser, _) = RecordingBrowser::new();
    AuthTypes::standard(transport, Arc::new(browser), keychain.map(|keychain| keychain as _))
}

fn oauth(url: &str, extra: &Value) -> Value {
    let mut settings = json!({"tokenUrl": url, "clientId": "cli", "clientSecret": "s3cret-value"});
    settings.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
    settings
}

#[tokio::test]
async fn every_written_type_meets_the_auth_provider_contract() {
    let types = types(None);
    let cases = [
        ("basic", json!({"username": "u", "password": "pa55word"}), json!({"username": "u"})),
        ("bearer", json!({"token": "t0k3n-value"}), json!({})),
        (
            "apiKey",
            json!({"name": "X-Key", "value": "k3y-value"}),
            json!({"name": "k", "value": "k3y-value", "in": "cookie"}),
        ),
    ];
    for (kind, served, refused) in cases {
        auth_provider_suite(&types, &request(kind, &served), &request(kind, &refused)).await;
    }
    let flow = Arc::new(FixedFlow::new(Ok(Credentials::header("authorization", "Bearer f"))));
    auth_provider_suite(&types, &request("flow", &json!({})).with_flow(flow), &request("flow", &json!({}))).await;
    auth_provider_suite(&types, &request("bearer", &json!({"token": "t0k3n"})), &request("saml", &json!({}))).await;
}

#[tokio::test]
async fn every_oauth_grant_meets_the_auth_provider_contract() {
    let types = types(None);
    let extra = json!({"username": "u", "password": "pa55word", "refreshToken": "r3fresh-value"});
    for kind in ["oauth2ClientCredentials", "oauth2Password", "oauth2Refresh"] {
        let served = request(kind, &oauth("https://id.test/token", &extra));
        auth_provider_suite(&types, &served, &request(kind, &oauth("https://id.test/refuse", &extra))).await;
    }
    let keychain = Arc::new(MemoryKeychain::new());
    keychain.set("vibranium", "api@cli", &Secret::new("kept")).unwrap();
    let types = self::types(Some(keychain));
    let kind = "oauth2AuthorizationCode";
    let served = request(kind, &oauth("https://id.test/token", &json!({})));
    let refused = request(kind, &oauth("https://id.test/token", &json!({"clientId": "other"})));
    auth_provider_suite(&types, &served, &refused).await;
}

#[tokio::test]
async fn every_key_a_type_requires_is_one_the_compiler_checks_for() {
    let keychain = Arc::new(MemoryKeychain::new());
    keychain.set("vibranium", "api@cli", &Secret::new("kept")).unwrap();
    let types = types(Some(keychain));
    let flow = Arc::new(FixedFlow::new(Ok(Credentials::header("authorization", "Bearer f"))));
    let value = |key: &str| if key.ends_with("Url") { "https://id.test/token" } else { "cli" };
    for (kind, needs) in vc_model::AUTH_NEEDS.iter().filter(|(kind, _)| *kind != "plugin") {
        let settings: serde_json::Map<String, Value> =
            needs.iter().map(|key| ((*key).to_owned(), json!(value(key)))).collect();
        let request = request(kind, &Value::Object(settings)).with_flow(Arc::clone(&flow) as _);
        let answer = types.credentials(&request).await;
        assert!(
            !matches!(answer, Err(AuthError::Missing { .. })),
            "{kind} requires a key AUTH_NEEDS leaves out: {answer:?}"
        );
    }
}

/// The keys a type reads only on the path that uses them: an authorization code reads its
/// `authorizeUrl` and `tokenUrl` only after the keychain had no refresh token. The compiler's
/// V1005 still asks for them.
const READ_LATE: [(&str, &str); 2] =
    [("oauth2AuthorizationCode", "authorizeUrl"), ("oauth2AuthorizationCode", "tokenUrl")];

#[tokio::test]
async fn leaving_out_any_key_a_type_needs_is_missing_that_key() {
    let types = types(None);
    let flow = Arc::new(FixedFlow::new(Ok(Credentials::header("authorization", "Bearer f"))));
    let value = |key: &str| if key.ends_with("Url") { "https://id.test/token" } else { "cli" };
    for (kind, needs) in vc_model::AUTH_NEEDS.iter().filter(|(kind, _)| *kind != "plugin") {
        for left in needs.iter().filter(|key| !READ_LATE.contains(&(*kind, **key))) {
            let kept = needs.iter().filter(|key| *key != left);
            let settings: serde_json::Map<String, Value> =
                kept.map(|key| ((*key).to_owned(), json!(value(key)))).collect();
            let mut request = request(kind, &Value::Object(settings));
            if *left != "steps" {
                request = request.with_flow(Arc::clone(&flow) as _);
            }
            let answer = types.credentials(&request).await;
            let missing = matches!(&answer, Err(AuthError::Missing { key, .. }) if key == left);
            assert!(missing, "{kind} without `{left}` gives {answer:?}");
        }
    }
}

#[tokio::test]
async fn the_fixed_types_build_their_headers_and_query() {
    let types = types(None);
    let basic = types.credentials(&request("basic", &json!({"username": "u", "password": "p"}))).await.unwrap();
    assert_eq!(basic, Credentials::header("authorization", "Basic dTpw"));
    let bearer = types.credentials(&request("bearer", &json!({"token": "t"}))).await.unwrap();
    assert_eq!(bearer, Credentials::header("authorization", "Bearer t"));
    let header = types.credentials(&request("apiKey", &json!({"name": "X-Key", "value": "v", "in": "Header"}))).await;
    assert_eq!(header.unwrap(), Credentials::header("x-key", "v"));
    let query = types.credentials(&request("apiKey", &json!({"name": "Key", "value": "v", "in": "QUERY"}))).await;
    assert_eq!(query.unwrap(), Credentials::query("Key", "v"));
    let missing = types.credentials(&request("flow", &json!({}))).await.err();
    assert_eq!(missing, Some(AuthError::Missing { auth: "api".into(), key: "steps".into() }));
    assert_eq!(format!("{types:?}").matches(", ").count(), 7, "eight types: {types:?}");
    assert!(format!("{types:?}").contains("oauth2AuthorizationCode"));
}

#[tokio::test]
async fn each_grant_posts_its_fields_and_scopes() {
    let (transport, recordings) = token_server(TOKEN);
    let (browser, _) = RecordingBrowser::new();
    let types = AuthTypes::standard(transport, Arc::new(browser), None);
    let extra = json!({"username": "u", "password": "pw", "refreshToken": "rt", "scopes": ["a", "b"]});
    for kind in ["oauth2ClientCredentials", "oauth2Password", "oauth2Refresh"] {
        types.credentials(&request(kind, &oauth("https://id.test/token", &extra))).await.unwrap();
    }
    let sent: Vec<_> = recordings.take().into_iter().map(|recorded| form(&recorded.request)).collect();
    let expected =
        [("client_credentials", None, None), ("password", Some("u"), None), ("refresh_token", None, Some("rt"))];
    for (fields, (grant, username, refresh)) in sent.iter().zip(expected) {
        assert_eq!(field(fields, "grant_type"), Some(grant));
        assert_eq!(field(fields, "scope"), Some("a b"));
        assert_eq!(field(fields, "username"), username);
        assert_eq!(field(fields, "refresh_token"), refresh);
    }
    assert_eq!(field(&sent[1], "password"), Some("pw"));
    let no_refresh = request("oauth2Refresh", &oauth("https://id.test/token", &json!({})));
    assert!(
        matches!(types.credentials(&no_refresh).await, Err(AuthError::Missing { ref key, .. }) if key == "refreshToken")
    );
}
