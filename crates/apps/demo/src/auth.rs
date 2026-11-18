//! The demo's sign-in routes (D331): `POST /auth/token` logs in, `POST /oauth/token` answers an
//! OAuth 2.0 client-credentials grant for the client `vc-client` with the secret `vc-secret`, and
//! `GET /auth/me` needs a token from either; `POST /auth/logout` revokes the one it is sent.
//! Tokens are `demo-token-1`, `demo-token-2`, … from one counter, not random ones (the playground
//! runs this router too), and only the newest [`TOKEN_WINDOW`] of them are valid, so the demo keeps
//! no table that grows. Nothing here logs.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::rejection::FormRejection;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::scripted::{Params, Refused};

/// How many of the newest tokens are valid; an older one gets 401.
pub const TOKEN_WINDOW: u64 = 1024;

/// What every token starts with, before its number.
const PREFIX: &str = "demo-token-";

/// The credentials of the one OAuth client, sent with the `Basic` scheme: `vc-client:vc-secret`.
const CLIENT: &str = "dmMtY2xpZW50OnZjLXNlY3JldA==";

/// How long a token says it lasts, in seconds.
const EXPIRES_IN: u64 = 3600;

/// The tokens handed out so far, and the ones revoked.
struct Tokens {
    issued: AtomicU64,
    /// Slot `n % TOKEN_WINDOW` holds the highest token number of that slot that was revoked, or 0.
    /// Taking the highest means an old token's revoke never clears or hits a newer token's, and a
    /// slot above `n` means `n` has left the window anyway.
    revoked: Box<[AtomicU64]>,
}

impl Default for Tokens {
    fn default() -> Self {
        Self { issued: AtomicU64::new(0), revoked: (0..TOKEN_WINDOW).map(|_| AtomicU64::new(0)).collect() }
    }
}

impl Tokens {
    fn issue(&self) -> String {
        format!("{PREFIX}{}", self.issued.fetch_add(1, Ordering::Relaxed).saturating_add(1))
    }

    /// The number of `token` when it is one of the newest [`TOKEN_WINDOW`] handed out and not
    /// revoked.
    fn valid(&self, token: &str) -> Option<u64> {
        let issued = self.issued.load(Ordering::Relaxed);
        let number = token
            .strip_prefix(PREFIX)
            .and_then(|digits| digits.parse::<u64>().ok().filter(|n| n.to_string() == digits));
        let live = number.filter(|&n| n >= 1 && n <= issued && issued - n < TOKEN_WINDOW);
        live.filter(|&n| self.slot(n).load(Ordering::Relaxed) != n)
    }

    /// Revokes token `number`, keeping a newer revoke of its slot.
    fn revoke(&self, number: u64) {
        self.slot(number).fetch_max(number, Ordering::Relaxed);
    }

    fn slot(&self, number: u64) -> &AtomicU64 {
        let index = usize::try_from(number % TOKEN_WINDOW).unwrap_or_default();
        &self.revoked[index]
    }
}

/// The sign-in routes, sharing one counter.
pub fn routes() -> Router {
    Router::new()
        .route("/auth/token", post(login))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/oauth/token", post(grant))
        .with_state(Arc::new(Tokens::default()))
}

/// The answer that hands out a new token.
fn issued(tokens: &Tokens) -> Json<Value> {
    Json(json!({"access_token": tokens.issue(), "token_type": "Bearer", "expires_in": EXPIRES_IN}))
}

/// `POST /auth/token`: a new token, whatever the body.
async fn login(State(tokens): State<Arc<Tokens>>) -> Json<Value> {
    issued(&tokens)
}

/// `GET /auth/me`: `{"user": "demo"}` with a valid `Bearer` token, else 401.
async fn me(State(tokens): State<Arc<Tokens>>, headers: HeaderMap) -> Response {
    match bearer(&tokens, &headers) {
        Some(_) => Json(json!({"user": "demo"})).into_response(),
        None => unauthorized(),
    }
}

/// `POST /auth/logout`: revokes the valid `Bearer` token it is sent (204), else 401.
async fn logout(State(tokens): State<Arc<Tokens>>, headers: HeaderMap) -> Response {
    let Some(number) = bearer(&tokens, &headers) else { return unauthorized() };
    tokens.revoke(number);
    StatusCode::NO_CONTENT.into_response()
}

/// The number of the valid `Bearer` token in `headers`, if there is one.
fn bearer(tokens: &Tokens, headers: &HeaderMap) -> Option<u64> {
    credentials(headers, "Bearer").and_then(|token| tokens.valid(token))
}

/// What the `Authorization` header in `headers` sends with `scheme`, whose name is matched in
/// any case (RFC 7235); what follows it is kept exactly.
fn credentials<'a>(headers: &'a HeaderMap, scheme: &str) -> Option<&'a str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (named, rest) = value.split_once(' ')?;
    named.eq_ignore_ascii_case(scheme).then_some(rest)
}

/// The 401 for a missing, unknown, old or revoked token.
fn unauthorized() -> Response {
    let challenge = [(header::WWW_AUTHENTICATE, "Bearer")];
    let refused = Refused("a valid bearer token is needed".to_owned());
    (challenge, refused.with_status(StatusCode::UNAUTHORIZED)).into_response()
}

/// `POST /oauth/token`: a new token for `vc-client`'s `client_credentials` grant, else the
/// OAuth error: `invalid_client` (401), `invalid_request` or `unsupported_grant_type` (400).
async fn grant(
    State(tokens): State<Arc<Tokens>>,
    headers: HeaderMap,
    form: Result<Form<Params>, FormRejection>,
) -> Response {
    if credentials(&headers, "Basic") != Some(CLIENT) {
        let challenge = [(header::WWW_AUTHENTICATE, "Basic")];
        let refused = Refused("invalid_client".to_owned());
        return (challenge, refused.with_status(StatusCode::UNAUTHORIZED)).into_response();
    }
    let Ok(Form(form)) = form else { return Refused("invalid_request".to_owned()).into_response() };
    if form.get("grant_type").is_none_or(|grant| grant != "client_credentials") {
        return Refused("unsupported_grant_type".to_owned()).into_response();
    }
    issued(&tokens).into_response()
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
