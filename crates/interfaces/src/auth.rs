//! Obtaining credentials for an `auth` definition (LLD "Auth and secrets", D106).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Map, Value};
use vc_diagnostics::Code;

use crate::Secret;

/// One `auth` definition with its settings rendered, ready for its type's provider.
///
/// The settings hold resolved secrets, so `Debug` shows only their keys.
#[derive(Clone)]
#[non_exhaustive]
pub struct AuthRequest {
    /// The definition's ID, for messages.
    pub auth: String,
    /// Its `type`, such as `oauth2ClientCredentials`.
    pub kind: String,
    /// Its other keys, rendered: `tokenUrl`, `clientId`, `scopes` and the rest.
    pub settings: Map<String, Value>,
    /// For a `flow`: its steps and `apply`, bound to this definition's vars by the executor.
    pub flow: Option<Arc<dyn ActionRunner>>,
}

impl AuthRequest {
    /// A request for definition `auth` of type `kind` with `settings` and no flow.
    #[must_use]
    pub fn new(auth: impl Into<String>, kind: impl Into<String>, settings: Map<String, Value>) -> Self {
        Self { auth: auth.into(), kind: kind.into(), settings, flow: None }
    }

    /// This request, with the flow that runs its steps.
    #[must_use]
    pub fn with_flow(self, flow: Arc<dyn ActionRunner>) -> Self {
        Self { flow: Some(flow), ..self }
    }

    /// The setting `key` as text, when it is a string.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.settings.get(key).and_then(Value::as_str)
    }
}

impl fmt::Debug for AuthRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthRequest")
            .field("auth", &self.auth)
            .field("kind", &self.kind)
            .field("settings", &self.settings.keys().collect::<Vec<_>>())
            .field("flow", &self.flow.is_some())
            .finish()
    }
}

/// What an auth adds to each request, and how long it holds. Every value is a [`Secret`], so
/// `Debug` hides them and the redactor masks them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Credentials {
    /// Headers to set, by name.
    pub headers: Vec<(String, Secret)>,
    /// Query parameters to add.
    pub query: Vec<(String, Secret)>,
    /// Cookies to send.
    pub cookies: Vec<(String, Secret)>,
    /// How long they hold from now; `None` when they never expire.
    pub expires_in: Option<Duration>,
}

impl Credentials {
    /// Credentials adding the header `name: value`.
    #[must_use]
    pub fn header(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self { headers: vec![(name.into(), Secret::new(value))], ..Self::default() }
    }

    /// Credentials adding the query parameter `name=value`.
    #[must_use]
    pub fn query(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self { query: vec![(name.into(), Secret::new(value))], ..Self::default() }
    }

    /// These credentials, expiring `after` from now.
    #[must_use]
    pub fn expiring(self, after: Option<Duration>) -> Self {
        Self { expires_in: after, ..self }
    }

    /// Every value they hold, for the redactor.
    pub fn values(&self) -> impl Iterator<Item = &str> {
        let all = self.headers.iter().chain(&self.query).chain(&self.cookies);
        all.map(|(_, value)| value.expose())
    }
}

/// Obtains credentials for one `auth` type (D106). The auth adapter's registry is one too,
/// handing each request to the entry for its `kind`.
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Credentials for `request`.
    ///
    /// # Errors
    ///
    /// [`AuthError`] when they cannot be obtained; it never holds a secret or a token body.
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError>;
}

/// Runs a `flow` definition's steps and renders its `apply` with what they saved.
#[async_trait]
pub trait ActionRunner: Send + Sync {
    /// The credentials the flow produces.
    ///
    /// # Errors
    ///
    /// [`AuthError::Flow`] when a step fails.
    async fn run(&self) -> Result<Credentials, AuthError>;
}

/// Why credentials could not be obtained. All are V5114, each with its own help; no variant
/// holds a secret value or a token response body.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AuthError {
    /// The token endpoint answered with an error status.
    #[error("the token endpoint for `{auth}` answered {status}")]
    Rejected {
        /// The definition's ID.
        auth: String,
        /// The status it answered with.
        status: u16,
    },
    /// The token endpoint could not be reached.
    #[error("cannot reach the token endpoint for `{auth}`: {reason}")]
    Unreachable {
        /// The definition's ID.
        auth: String,
        /// The transport's reason.
        reason: String,
    },
    /// The token endpoint answered 2xx without a usable token.
    #[error("the token response for `{auth}` has no {missing}")]
    Malformed {
        /// The definition's ID.
        auth: String,
        /// What it lacks, such as `access_token`.
        missing: String,
    },
    /// A setting the type needs is not set or not text.
    #[error("`{auth}` needs `{key}`")]
    Missing {
        /// The definition's ID.
        auth: String,
        /// The setting's key.
        key: String,
    },
    /// No provider handles the type.
    #[error("no auth type is called `{kind}`")]
    Unknown {
        /// The type as written.
        kind: String,
    },
    /// The browser sign-in did not complete.
    #[error("the browser sign-in for `{auth}` did not complete: {reason}")]
    Browser {
        /// The definition's ID.
        auth: String,
        /// What went wrong, such as a state mismatch or a timeout.
        reason: String,
    },
    /// A `flow` step failed.
    #[error("the `{auth}` flow failed: {reason}")]
    Flow {
        /// The definition's ID.
        auth: String,
        /// The step's failure, redacted.
        reason: String,
    },
}

impl AuthError {
    /// The diagnostic code, V5114 for every cause.
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::V5114
    }

    /// Whether the token endpoint refused the grant itself: 400 (RFC 6749 section 5.2, such as
    /// `invalid_grant` or `invalid_client`), 401 or 403. Asking again with the same grant gets
    /// the same answer, and may lock the account.
    #[must_use]
    pub const fn refused(&self) -> bool {
        matches!(self, Self::Rejected { status: 400 | 401 | 403, .. })
    }

    /// Whether the failure may pass by itself: the endpoint could not be reached (no route, a
    /// timeout) or answered 408, 429 or 5xx, so asking again later can succeed.
    #[must_use]
    pub const fn may_pass(&self) -> bool {
        matches!(self, Self::Unreachable { .. } | Self::Rejected { status: 408 | 429 | 500..=599, .. })
    }

    /// What the user can do about it.
    #[must_use]
    pub const fn help(&self) -> &'static str {
        match self {
            Self::Rejected { .. } => "check the client ID, the secret and the scopes the definition sends",
            Self::Unreachable { .. } => "check `tokenUrl` and that the identity server is up",
            Self::Malformed { .. } => "check `tokenUrl` points at an OAuth token endpoint",
            Self::Missing { .. } => "add the key to the definition, or a var or secret that sets it",
            Self::Unknown { .. } => "use one of the auth types in the syntax reference",
            Self::Browser { .. } => "sign in again; the browser must reach the localhost redirect",
            Self::Flow { .. } => "run the flow's steps on their own and check each one's response",
        }
    }
}

#[cfg(test)]
#[path = "auth_tests.rs"]
mod tests;
