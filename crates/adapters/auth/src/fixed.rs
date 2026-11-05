//! The types whose credentials are written in the definition: `basic`, `bearer`, `apiKey`,
//! and `flow`, whose steps the executor runs.

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::header::AUTHORIZATION;
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, Credentials};

use crate::token::required;

/// `basic`: `authorization: Basic base64(username:password)`.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Basic;

/// `bearer`: `authorization: Bearer <token>`.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Bearer;

/// `apiKey`: `value` in the header or query parameter `name`, as `in` says.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ApiKey;

/// `flow`: the credentials its steps and `apply` give (D106).
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Flow;

#[async_trait]
impl AuthProvider for Basic {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let pair = format!("{}:{}", required(request, "username")?, required(request, "password")?);
        Ok(Credentials::header(AUTHORIZATION.as_str(), format!("Basic {}", STANDARD.encode(pair))))
    }
}

#[async_trait]
impl AuthProvider for Bearer {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        Ok(Credentials::header(AUTHORIZATION.as_str(), format!("Bearer {}", required(request, "token")?)))
    }
}

#[async_trait]
impl AuthProvider for ApiKey {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let (name, value) = (required(request, "name")?, required(request, "value")?);
        let place = request.text("in").unwrap_or("header");
        if place.eq_ignore_ascii_case("query") {
            return Ok(Credentials::query(name, value));
        }
        if place.eq_ignore_ascii_case("header") {
            return Ok(Credentials::header(name.to_ascii_lowercase(), value));
        }
        Err(AuthError::Missing { auth: request.auth.clone(), key: "in: header or query".to_owned() })
    }
}

#[async_trait]
impl AuthProvider for Flow {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let missing = || AuthError::Missing { auth: request.auth.clone(), key: "steps".to_owned() };
        request.flow.as_ref().ok_or_else(missing)?.run().await
    }
}
