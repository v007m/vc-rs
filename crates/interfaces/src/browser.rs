//! Opening the user's browser, for `oauth2AuthorizationCode` sign-in.

use async_trait::async_trait;

/// Opens a URL in the user's browser. Tests fake it by following the redirect themselves.
#[async_trait]
pub trait Browser: Send + Sync {
    /// Opens `url`.
    ///
    /// # Errors
    ///
    /// [`BrowserError`] when no browser could be started.
    async fn open(&self, url: &str) -> Result<(), BrowserError>;
}

/// Why the browser could not be opened.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot open a browser: {reason}")]
#[non_exhaustive]
pub struct BrowserError {
    /// What went wrong.
    pub reason: String,
}

impl BrowserError {
    /// An error for `reason`.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}
