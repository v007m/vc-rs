//! The auth layer of the pipeline (D141): it sits inside the retry, so every attempt puts the
//! current credentials on the request, and a `refreshOn` response drops them before the retry.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use http::header::COOKIE;
use http::{HeaderName, HeaderValue, Uri};
use vc_interfaces::{AuthError, AuthProvider, Credentials, Secret, TransportError};
use vc_model::RenderedRequest;

use super::{Lease, Prepared, TokenCache};
use crate::action::request;
use crate::{Attempted, Redactor, RequestStage, RetryPolicy};

/// Puts the credentials of one auth on each request, fetching them through the run's
/// [`TokenCache`] under the call's [`Lease`]; whatever it fetches is masked from then on.
pub struct AuthStage {
    prepared: Prepared,
    tokens: Arc<TokenCache>,
    lease: Arc<Lease>,
    provider: Arc<dyn AuthProvider>,
    redactor: Arc<Redactor>,
}

impl fmt::Debug for AuthStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthStage").field("auth", &self.prepared.request).finish_non_exhaustive()
    }
}

impl AuthStage {
    pub fn new(
        prepared: Prepared,
        (tokens, lease): (Arc<TokenCache>, Arc<Lease>),
        (provider, redactor): (Arc<dyn AuthProvider>, Arc<Redactor>),
    ) -> Self {
        Self { prepared, tokens, lease, provider, redactor }
    }

    async fn fetch(&self) -> Result<Credentials, AuthError> {
        let credentials = self.provider.credentials(&self.prepared.request).await?;
        self.redactor.hide_values(hidden(&credentials));
        self.redactor.hide_headers(credentials.headers.iter().map(|(name, _)| name));
        Ok(credentials)
    }
}

#[async_trait]
impl RequestStage for AuthStage {
    async fn apply(&self, request: RenderedRequest) -> Result<RenderedRequest, TransportError> {
        let before = self.prepared.refresh_before;
        let credentials =
            self.tokens.get(&self.lease, before, || self.fetch()).await.map_err(TransportError::Credentials)?;
        attach(request, &credentials).ok_or_else(|| {
            let (auth, missing) = (self.prepared.request.auth.clone(), "usable header, cookie or query value".into());
            TransportError::Credentials(AuthError::Malformed { auth, missing })
        })
    }
}

/// What the redactor must mask once `credentials` arrive: each value, and the token after a
/// scheme such as `Bearer `, which can show up on its own (in the response that issued it).
fn hidden(credentials: &Credentials) -> Vec<&str> {
    let tokens = credentials.values().filter_map(|value| value.split_once(' ').map(|(_, token)| token.trim()));
    credentials.values().chain(tokens).collect()
}

/// `request` with `credentials` on it, or `None` when one of them cannot go in a request.
fn attach(mut request: RenderedRequest, credentials: &Credentials) -> Option<RenderedRequest> {
    for (name, value) in &credentials.headers {
        request.headers.insert(HeaderName::try_from(name.as_str()).ok()?, sensitive(value.expose())?);
    }
    if !credentials.cookies.is_empty() {
        let cookie = cookies(request.headers.get(COOKIE), &credentials.cookies);
        request.headers.insert(COOKIE, sensitive(&cookie)?);
    }
    if !credentials.query.is_empty() {
        request.uri = with_query(&request.uri, &credentials.query)?;
    }
    Some(request)
}

fn sensitive(value: &str) -> Option<HeaderValue> {
    let mut value = HeaderValue::try_from(value).ok()?;
    value.set_sensitive(true);
    Some(value)
}

/// The request's own cookies, then the credentials', as one `Cookie` header.
fn cookies(own: Option<&HeaderValue>, added: &[(String, Secret)]) -> String {
    let own = own.and_then(|value| value.to_str().ok()).filter(|own| !own.is_empty());
    let added = added.iter().map(|(name, value)| format!("{name}={}", value.expose()));
    own.map(str::to_owned).into_iter().chain(added).collect::<Vec<_>>().join("; ")
}

fn with_query(uri: &Uri, added: &[(String, Secret)]) -> Option<Uri> {
    let mut url = uri.to_string();
    for (name, value) in added {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&request::encode(name));
        url.push('=');
        url.push_str(&request::encode(value.expose()));
    }
    url.parse().ok()
}

/// The retry of a call that uses auth: when a response has one of the `refreshOn` statuses
/// it drops the credentials the call used, unless another request has already fetched them
/// again, and retries at once, a single time per call; otherwise the call's own retry decides.
pub struct RefreshOn {
    inner: Arc<dyn RetryPolicy>,
    tokens: Arc<TokenCache>,
    lease: Arc<Lease>,
    statuses: Vec<u16>,
    fired: AtomicBool,
}

impl fmt::Debug for RefreshOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RefreshOn").field("statuses", &self.statuses).finish_non_exhaustive()
    }
}

impl RefreshOn {
    pub fn new(
        inner: Arc<dyn RetryPolicy>,
        (tokens, lease): (Arc<TokenCache>, Arc<Lease>),
        statuses: Vec<u16>,
    ) -> Self {
        Self { inner, tokens, lease, statuses, fired: AtomicBool::new(false) }
    }
}

impl RetryPolicy for RefreshOn {
    fn delay(&self, attempt: &Attempted<'_>) -> Option<Duration> {
        let status = attempt.result.as_ref().ok().map(|exchange| exchange.response.status.as_u16());
        if status.is_some_and(|status| self.statuses.contains(&status)) && !self.fired.swap(true, Ordering::SeqCst) {
            self.tokens.invalidate(&self.lease);
            return Some(Duration::ZERO);
        }
        self.inner.delay(attempt)
    }
}
