//! The OAuth 2.0 token endpoint (RFC 6749 section 3.2), shared by every `oauth2*` type.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use bytes::Bytes;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderValue, Method, Uri};
use serde_json::Value;
use vc_interfaces::{AuthError, AuthRequest, Credentials, SendContext, Transport};
use vc_model::RenderedRequest;

const FORM: &str = "application/x-www-form-urlencoded";

/// `key=value` pairs, form-encoded as a form body is (D273), joined with `&`.
pub fn encode(pairs: &[(&str, &str)]) -> String {
    vc_model::form_pairs(pairs.iter().copied())
}

/// The setting `key` as text.
///
/// # Errors
///
/// [`AuthError::Missing`] when it is not set or not text.
pub fn required<'r>(request: &'r AuthRequest, key: &str) -> Result<&'r str, AuthError> {
    request.text(key).ok_or_else(|| AuthError::Missing { auth: request.auth.clone(), key: key.to_owned() })
}

/// `scopes` as one space-separated value: a list or a string as written.
pub fn scopes(request: &AuthRequest) -> Option<String> {
    match request.settings.get("scopes")? {
        Value::String(scopes) => Some(scopes.clone()),
        Value::Array(scopes) => Some(scopes.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" ")),
        _ => None,
    }
}

/// A token from the endpoint: the access token and, when it gave one, the refresh token.
#[derive(Clone)]
pub struct Token {
    /// The credentials the access token gives.
    pub credentials: Credentials,
    /// The refresh token, for `oauth2AuthorizationCode` to keep.
    #[cfg_attr(target_family = "wasm", expect(dead_code, reason = "the WebAssembly build has no browser sign-in"))]
    pub refresh: Option<String>,
}

/// Posts a grant to the definition's `tokenUrl`: the client authenticates with HTTP Basic when
/// it has a `clientSecret` and by `client_id` in the form when it is public.
pub struct TokenEndpoint {
    transport: Arc<dyn Transport>,
}

impl TokenEndpoint {
    /// An endpoint reached through `transport`.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }

    /// The token for `grant`, a list of form fields.
    ///
    /// # Errors
    ///
    /// [`AuthError`] when the endpoint cannot be reached, refuses or gives no access token.
    pub async fn token(&self, request: &AuthRequest, grant: &[(&str, &str)]) -> Result<Token, AuthError> {
        let sent = post(request, grant)?;
        let auth = || request.auth.clone();
        let exchange = self.transport.send(sent, &SendContext::default()).await;
        let exchange = exchange.map_err(|error| AuthError::Unreachable { auth: auth(), reason: error.to_string() })?;
        let status = exchange.response.status;
        if !status.is_success() {
            return Err(AuthError::Rejected { auth: auth(), status: status.as_u16() });
        }
        parse(request, &exchange.response.body)
    }
}

fn post(request: &AuthRequest, grant: &[(&str, &str)]) -> Result<RenderedRequest, AuthError> {
    let url = required(request, "tokenUrl")?;
    let invalid = || AuthError::Missing { auth: request.auth.clone(), key: "tokenUrl".to_owned() };
    let uri = Uri::try_from(url).map_err(|_| invalid())?;
    let client = required(request, "clientId")?;
    let mut fields = grant.to_vec();
    if let Some(audience) = request.text("audience") {
        fields.push(("audience", audience));
    }
    let mut sent = RenderedRequest::new(Method::POST, uri);
    match request.text("clientSecret") {
        Some(secret) => {
            let pair = format!("{}:{}", encode_one(client), encode_one(secret));
            let basic = HeaderValue::try_from(format!("Basic {}", STANDARD.encode(pair))).map_err(|_| invalid())?;
            sent.headers.insert(AUTHORIZATION, basic);
        }
        None => fields.push(("client_id", client)),
    }
    sent.headers.insert(CONTENT_TYPE, HeaderValue::from_static(FORM));
    sent.headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    sent.body = Bytes::from(encode(&fields));
    Ok(sent)
}

/// RFC 6749 section 2.3.1: the client ID and secret are form-encoded before Basic encoding.
fn encode_one(text: &str) -> String {
    vc_model::form_text(text)
}

fn parse(request: &AuthRequest, body: &[u8]) -> Result<Token, AuthError> {
    let malformed = |missing: &str| AuthError::Malformed { auth: request.auth.clone(), missing: missing.to_owned() };
    let parsed: Value = serde_json::from_slice(body).map_err(|_| malformed("JSON body"))?;
    let access = parsed.get("access_token").and_then(Value::as_str).ok_or_else(|| malformed("access_token"))?;
    let kind = parsed.get("token_type").and_then(Value::as_str).unwrap_or("Bearer");
    let kind = if kind.eq_ignore_ascii_case("bearer") { "Bearer" } else { kind };
    let expires_in = parsed.get("expires_in").and_then(seconds).map(Duration::from_secs);
    let credentials = Credentials::header(AUTHORIZATION.as_str(), format!("{kind} {access}")).expiring(expires_in);
    let refresh = parsed.get("refresh_token").and_then(Value::as_str).map(str::to_owned);
    Ok(Token { credentials, refresh })
}

/// `expires_in` as a number, or as a string some servers send.
fn seconds(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;
