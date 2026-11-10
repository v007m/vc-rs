//! `oauth2AuthorizationCode`: a browser sign-in with PKCE (RFC 7636) and a loopback redirect
//! (RFC 8252), with the refresh token kept in the keychain so later runs skip the browser.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use tokio::net::TcpListener;
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, Browser, Credentials, Keychain, Secret, Transport};

use crate::redirect;
use crate::token::{TokenEndpoint, encode, required, scopes};

/// The keychain service the refresh tokens are kept under.
const SERVICE: &str = "vibranium";

/// How long the user has to finish signing in.
const SIGN_IN: Duration = Duration::from_secs(300);

/// The browser sign-in, its token exchange and the keychain that keeps its refresh token.
pub struct AuthorizationCode {
    endpoint: TokenEndpoint,
    browser: Arc<dyn Browser>,
    keychain: Option<Arc<dyn Keychain>>,
}

/// One sign-in's PKCE verifier and challenge, and the `state` that ties the redirect to it.
struct Pkce {
    verifier: String,
    challenge: String,
    state: String,
}

impl AuthorizationCode {
    /// The type, posting through `transport`, signing in through `browser` and keeping refresh
    /// tokens in `keychain` when there is one.
    pub fn new(transport: Arc<dyn Transport>, browser: Arc<dyn Browser>, keychain: Option<Arc<dyn Keychain>>) -> Self {
        Self { endpoint: TokenEndpoint::new(transport), browser, keychain }
    }

    /// Credentials from the kept refresh token, or `None` when none is kept or the server
    /// refused it, which is then forgotten. A failure that may pass keeps the token.
    async fn refreshed(&self, request: &AuthRequest, account: &str) -> Result<Option<Credentials>, AuthError> {
        let Some(keychain) = self.keychain.as_ref() else { return Ok(None) };
        let Some(kept) = keychain.get(SERVICE, account).ok().flatten() else { return Ok(None) };
        let grant = [("grant_type", "refresh_token"), ("refresh_token", kept.expose())];
        match self.endpoint.token(request, &grant).await {
            Ok(token) => {
                self.keep(account, token.refresh.as_deref());
                Ok(Some(token.credentials))
            }
            Err(error) if error.refused() => {
                let _ = keychain.delete(SERVICE, account);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    fn keep(&self, account: &str, refresh: Option<&str>) {
        if let (Some(keychain), Some(refresh)) = (&self.keychain, refresh) {
            let _ = keychain.set(SERVICE, account, &Secret::new(refresh));
        }
    }

    async fn signed_in(&self, request: &AuthRequest) -> Result<(String, String, Pkce), AuthError> {
        let failed = |reason: String| AuthError::Browser { auth: request.auth.clone(), reason };
        let port = request.settings.get("redirectPort").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let port = u16::try_from(port).map_err(|_| failed("`redirectPort` is not a port".to_owned()))?;
        let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|error| failed(error.to_string()))?;
        let port = listener.local_addr().map_err(|error| failed(error.to_string()))?.port();
        let redirect_uri = format!("http://127.0.0.1:{port}/callback");
        let pkce = Pkce::new().ok_or_else(|| failed("no secure random source".to_owned()))?;
        let url = authorize_url(request, &redirect_uri, &pkce)?;
        self.browser.open(&url).await.map_err(|error| failed(error.to_string()))?;
        let received = tokio::time::timeout(SIGN_IN, redirect::receive(&listener, &pkce.state)).await;
        let code = received.map_err(|_| failed("no sign-in within 5 minutes".to_owned()))?.map_err(failed)?;
        Ok((code, redirect_uri, pkce))
    }
}

impl Pkce {
    fn new() -> Option<Self> {
        let verifier = random_text(32)?;
        let challenge = URL_SAFE_NO_PAD.encode(digest(&SHA256, verifier.as_bytes()));
        Some(Self { verifier, challenge, state: random_text(16)? })
    }
}

/// `bytes` secure random bytes as unpadded URL-safe base64.
fn random_text(bytes: usize) -> Option<String> {
    let mut drawn = vec![0; bytes];
    SystemRandom::new().fill(&mut drawn).ok()?;
    Some(URL_SAFE_NO_PAD.encode(drawn))
}

/// Whether `pkce` is on: it is unless the definition says `pkce: false`.
fn pkce_on(request: &AuthRequest) -> bool {
    request.settings.get("pkce").and_then(serde_json::Value::as_bool).unwrap_or(true)
}

fn authorize_url(request: &AuthRequest, redirect_uri: &str, pkce: &Pkce) -> Result<String, AuthError> {
    let base = required(request, "authorizeUrl")?;
    let scope = scopes(request);
    let mut pairs = vec![
        ("response_type", "code"),
        ("client_id", required(request, "clientId")?),
        ("redirect_uri", redirect_uri),
        ("state", pkce.state.as_str()),
    ];
    pairs.extend(scope.as_deref().map(|scope| ("scope", scope)));
    if pkce_on(request) {
        pairs.extend([("code_challenge", pkce.challenge.as_str()), ("code_challenge_method", "S256")]);
    }
    let joiner = if base.contains('?') { '&' } else { '?' };
    Ok(format!("{base}{joiner}{}", encode(&pairs)))
}

#[async_trait]
impl AuthProvider for AuthorizationCode {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let account = format!("{}@{}", request.auth, required(request, "clientId")?);
        if let Some(credentials) = self.refreshed(request, &account).await? {
            return Ok(credentials);
        }
        let (code, redirect_uri, pkce) = self.signed_in(request).await?;
        let mut grant = vec![("grant_type", "authorization_code"), ("code", code.as_str())];
        grant.push(("redirect_uri", redirect_uri.as_str()));
        if pkce_on(request) {
            grant.push(("code_verifier", pkce.verifier.as_str()));
        }
        let token = self.endpoint.token(request, &grant).await?;
        self.keep(&account, token.refresh.as_deref());
        Ok(token.credentials)
    }
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
