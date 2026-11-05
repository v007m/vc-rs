//! The OAuth 2.0 grants that need no browser: client credentials, password and refresh token.

use std::sync::Arc;

use async_trait::async_trait;
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, Credentials, Transport};

use crate::token::{TokenEndpoint, required, scopes};

/// `oauth2ClientCredentials`: `grant_type=client_credentials` with the definition's scopes.
pub struct ClientCredentials(TokenEndpoint);

/// `oauth2Password`: `grant_type=password` with `username` and `password`.
pub struct PasswordGrant(TokenEndpoint);

/// `oauth2Refresh`: `grant_type=refresh_token` with a long-lived `refreshToken`.
pub struct RefreshGrant(TokenEndpoint);

impl ClientCredentials {
    /// The grant, posted through `transport`.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self(TokenEndpoint::new(transport))
    }
}

impl PasswordGrant {
    /// The grant, posted through `transport`.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self(TokenEndpoint::new(transport))
    }
}

impl RefreshGrant {
    /// The grant, posted through `transport`.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self(TokenEndpoint::new(transport))
    }
}

/// `grant` with `scope` added when the definition has scopes.
fn with_scope<'g>(mut grant: Vec<(&'g str, &'g str)>, scope: Option<&'g str>) -> Vec<(&'g str, &'g str)> {
    grant.extend(scope.map(|scope| ("scope", scope)));
    grant
}

#[async_trait]
impl AuthProvider for ClientCredentials {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let scope = scopes(request);
        let grant = with_scope(vec![("grant_type", "client_credentials")], scope.as_deref());
        Ok(self.0.token(request, &grant).await?.credentials)
    }
}

#[async_trait]
impl AuthProvider for PasswordGrant {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let (username, password) = (required(request, "username")?, required(request, "password")?);
        let scope = scopes(request);
        let grant = vec![("grant_type", "password"), ("username", username), ("password", password)];
        Ok(self.0.token(request, &with_scope(grant, scope.as_deref())).await?.credentials)
    }
}

#[async_trait]
impl AuthProvider for RefreshGrant {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let refresh = required(request, "refreshToken")?;
        let scope = scopes(request);
        let grant = with_scope(vec![("grant_type", "refresh_token"), ("refresh_token", refresh)], scope.as_deref());
        Ok(self.0.token(request, &grant).await?.credentials)
    }
}
