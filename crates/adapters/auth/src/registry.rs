//! The auth types by `type` name (D106): one entry each, and no plugin types.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, Credentials, Transport};
#[cfg(not(target_family = "wasm"))]
use vc_interfaces::{Browser, Keychain};

#[cfg(not(target_family = "wasm"))]
use crate::AuthorizationCode;
use crate::{ApiKey, Basic, Bearer, ClientCredentials, Flow, PasswordGrant, RefreshGrant};

/// Every auth type, by the name `type:` gives it. It is a provider too: it hands each request
/// to the entry for its `kind`.
#[derive(Clone, Default)]
pub struct AuthTypes {
    by_kind: BTreeMap<String, Arc<dyn AuthProvider>>,
}

impl AuthTypes {
    /// No types.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The standard types: token endpoints are reached through `transport`, the browser
    /// sign-in opens `browser`, and refresh tokens are kept in `keychain` when there is one.
    #[cfg(not(target_family = "wasm"))]
    #[must_use]
    pub fn standard(
        transport: Arc<dyn Transport>,
        browser: Arc<dyn Browser>,
        keychain: Option<Arc<dyn Keychain>>,
    ) -> Self {
        let code = AuthorizationCode::new(Arc::clone(&transport), browser, keychain);
        Self::headless(transport).with("oauth2AuthorizationCode", Arc::new(code))
    }

    /// The standard types but the browser sign-in, `oauth2AuthorizationCode`: what a build with
    /// no browser to open and no port to listen on (the playground) can run.
    #[must_use]
    pub fn headless(transport: Arc<dyn Transport>) -> Self {
        Self::new()
            .with("basic", Arc::new(Basic))
            .with("bearer", Arc::new(Bearer))
            .with("apiKey", Arc::new(ApiKey))
            .with("flow", Arc::new(Flow))
            .with("oauth2ClientCredentials", Arc::new(ClientCredentials::new(Arc::clone(&transport))))
            .with("oauth2Password", Arc::new(PasswordGrant::new(Arc::clone(&transport))))
            .with("oauth2Refresh", Arc::new(RefreshGrant::new(transport)))
    }

    /// These types, with `kind` added or replaced.
    #[must_use]
    pub fn with(mut self, kind: &str, provider: Arc<dyn AuthProvider>) -> Self {
        self.by_kind.insert(kind.to_owned(), provider);
        self
    }
}

impl fmt::Debug for AuthTypes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_kind.keys()).finish()
    }
}

#[async_trait]
impl AuthProvider for AuthTypes {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let unknown = || AuthError::Unknown { kind: request.kind.clone() };
        self.by_kind.get(&request.kind).ok_or_else(unknown)?.credentials(request).await
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
