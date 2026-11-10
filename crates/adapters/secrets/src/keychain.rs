//! The OS keychain over `keyring-core` and one native store per system: Keychain Services on
//! macOS, the Credential Manager on Windows, the kernel keyutils session keyring on Linux.

use std::fmt;
use std::sync::Arc;

use keyring_core::{CredentialStore, Entry, Error};
use vc_interfaces::{Keychain, KeychainError, Secret};

/// A [`Keychain`] over a `keyring-core` credential store: the platform's, or any other.
#[derive(Clone)]
pub struct OsKeychain {
    store: Arc<CredentialStore>,
}

impl OsKeychain {
    /// The platform's keychain: the store of the system this was built for.
    ///
    /// # Errors
    ///
    /// [`KeychainError`] when this system has none, so the caller goes without.
    pub fn platform() -> Result<Self, KeychainError> {
        native().map(Self::new)
    }

    /// A keychain over `store`.
    #[must_use]
    pub fn new(store: Arc<CredentialStore>) -> Self {
        Self { store }
    }

    fn entry(&self, service: &str, account: &str) -> Result<Entry, KeychainError> {
        self.store.build(service, account, None).map_err(|failure| error(&failure))
    }
}

impl fmt::Debug for OsKeychain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OsKeychain").field("store", &self.store.vendor()).finish()
    }
}

impl Keychain for OsKeychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, KeychainError> {
        match self.entry(service, account)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(Error::NoEntry) => Ok(None),
            Err(failure) => Err(error(&failure)),
        }
    }

    fn set(&self, service: &str, account: &str, secret: &Secret) -> Result<(), KeychainError> {
        self.entry(service, account)?.set_password(secret.expose()).map_err(|failure| error(&failure))
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), KeychainError> {
        match self.entry(service, account)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(failure) => Err(error(&failure)),
        }
    }
}

/// Keychain Services, in the user's login keychain.
#[cfg(target_os = "macos")]
fn native() -> Result<Arc<CredentialStore>, KeychainError> {
    let store = apple_native_keyring_store::keychain::Store::new().map_err(|failure| error(&failure))?;
    Ok(store)
}

/// The Windows Credential Manager.
#[cfg(target_os = "windows")]
fn native() -> Result<Arc<CredentialStore>, KeychainError> {
    let store = windows_native_keyring_store::Store::new().map_err(|failure| error(&failure))?;
    Ok(store)
}

/// The kernel keyutils session keyring: it lasts for the login session only.
#[cfg(target_os = "linux")]
fn native() -> Result<Arc<CredentialStore>, KeychainError> {
    let store = linux_keyutils_keyring_store::Store::new().map_err(|failure| error(&failure))?;
    Ok(store)
}

/// No store is built for this system.
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn native() -> Result<Arc<CredentialStore>, KeychainError> {
    Err(KeychainError::new("no keychain on this system"))
}

/// The keychain's error as a reason. No variant's message holds the secret: a badly encoded
/// one is reported without its bytes.
fn error(failure: &Error) -> KeychainError {
    KeychainError::new(failure.to_string())
}

#[cfg(test)]
#[path = "keychain_tests.rs"]
mod tests;
