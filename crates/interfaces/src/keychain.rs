//! The OS keychain, where `oauth2AuthorizationCode` keeps its refresh token between runs.

use crate::Secret;

/// Keeps secrets in the OS keychain, by service and account.
pub trait Keychain: Send + Sync {
    /// The secret stored for `account` under `service`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// [`KeychainError`] when the keychain cannot be read.
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, KeychainError>;

    /// Stores `secret` for `account` under `service`, replacing any earlier one.
    ///
    /// # Errors
    ///
    /// [`KeychainError`] when the keychain cannot be written.
    fn set(&self, service: &str, account: &str, secret: &Secret) -> Result<(), KeychainError>;

    /// Removes the secret for `account` under `service`; removing none is fine.
    ///
    /// # Errors
    ///
    /// [`KeychainError`] when the keychain cannot be written.
    fn delete(&self, service: &str, account: &str) -> Result<(), KeychainError>;
}

/// Why the keychain could not be used. It never holds the secret.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the keychain is unavailable: {reason}")]
#[non_exhaustive]
pub struct KeychainError {
    /// What went wrong.
    pub reason: String,
}

impl KeychainError {
    /// An error for `reason`.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}
