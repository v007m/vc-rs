use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};
use vc_interfaces::{
    ActionRunner, AuthError, AuthProvider, AuthRequest, Browser, BrowserError, Credentials, Keychain, KeychainError,
    Secret,
};

/// An [`AuthProvider`] for the `kinds` it is given: each call gives `Bearer <kind>-<n>`,
/// counting from 1, so a test can see when credentials were obtained again.
#[derive(Clone, Debug, Default)]
pub struct FakeAuth {
    kinds: Vec<String>,
    calls: Arc<AtomicUsize>,
}

impl FakeAuth {
    /// A provider serving `kinds`; any other kind is [`AuthError::Unknown`].
    #[must_use]
    pub fn new(kinds: &[&str]) -> Self {
        Self { kinds: kinds.iter().map(|kind| (*kind).to_owned()).collect(), calls: Arc::default() }
    }

    /// How many credentials it gave or refused.
    #[must_use]
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AuthProvider for FakeAuth {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        if !self.kinds.contains(&request.kind) {
            return Err(AuthError::Unknown { kind: request.kind.clone() });
        }
        Ok(Credentials::header("authorization", format!("Bearer {}-{call}", request.kind)))
    }
}

/// An [`ActionRunner`] that gives the same outcome every time and counts its runs.
#[derive(Clone, Debug)]
pub struct FixedFlow {
    outcome: Result<Credentials, AuthError>,
    runs: Arc<AtomicUsize>,
}

impl FixedFlow {
    /// A flow giving `outcome`.
    #[must_use]
    pub fn new(outcome: Result<Credentials, AuthError>) -> Self {
        Self { outcome, runs: Arc::default() }
    }

    /// How many times it ran.
    #[must_use]
    pub fn runs(&self) -> usize {
        self.runs.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ActionRunner for FixedFlow {
    async fn run(&self) -> Result<Credentials, AuthError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.outcome.clone()
    }
}

/// A [`Browser`] that opens nothing: it sends each `http` URL down a channel, so a test can
/// follow the redirect itself.
#[derive(Clone, Debug)]
pub struct RecordingBrowser {
    opened: mpsc::UnboundedSender<String>,
}

impl RecordingBrowser {
    /// The browser and the receiving end of the URLs it opens.
    #[must_use]
    pub fn new() -> (Self, mpsc::UnboundedReceiver<String>) {
        let (opened, urls) = mpsc::unbounded_channel();
        (Self { opened }, urls)
    }
}

#[async_trait]
impl Browser for RecordingBrowser {
    async fn open(&self, url: &str) -> Result<(), BrowserError> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(BrowserError::new("only http and https URLs open"));
        }
        self.opened.send(url.to_owned()).map_err(|_| BrowserError::new("nobody is listening"))
    }
}

/// What a [`MemoryKeychain`] holds: a secret per service and account.
type Entries = BTreeMap<(String, String), Secret>;

/// A [`Keychain`] in memory, behind a watch channel rather than a lock.
pub struct MemoryKeychain {
    entries: watch::Sender<Entries>,
}

impl MemoryKeychain {
    /// An empty keychain.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: watch::Sender::new(BTreeMap::new()) }
    }
}

impl Default for MemoryKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for MemoryKeychain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.entries.borrow().keys()).finish()
    }
}

impl Keychain for MemoryKeychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<Secret>, KeychainError> {
        Ok(self.entries.borrow().get(&(service.to_owned(), account.to_owned())).cloned())
    }

    fn set(&self, service: &str, account: &str, secret: &Secret) -> Result<(), KeychainError> {
        let key = (service.to_owned(), account.to_owned());
        self.entries.send_modify(|entries| {
            entries.insert(key, secret.clone());
        });
        Ok(())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), KeychainError> {
        let key = (service.to_owned(), account.to_owned());
        self.entries.send_modify(|entries| {
            entries.remove(&key);
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Map;
    use vc_interfaces::{AuthError, AuthRequest, Credentials};

    use super::{FakeAuth, FixedFlow, MemoryKeychain, RecordingBrowser};
    use crate::{action_runner_suite, auth_provider_suite, browser_suite, keychain_suite};

    #[tokio::test]
    async fn fake_auth_meets_the_auth_provider_contract() {
        let auth = FakeAuth::new(&["bearer"]);
        let served = AuthRequest::new("a", "bearer", Map::new());
        auth_provider_suite(&auth, &served, &AuthRequest::new("a", "kerberos", Map::new())).await;
        assert_eq!(auth.calls(), 2);
    }

    #[tokio::test]
    async fn fixed_flows_meet_the_action_runner_contract() {
        let flow = FixedFlow::new(Ok(Credentials::header("authorization", "Bearer t")));
        action_runner_suite(&flow).await;
        assert_eq!(flow.runs(), 2);
        let failing = FixedFlow::new(Err(AuthError::Flow { auth: "a".into(), reason: "step failed".into() }));
        action_runner_suite(&failing).await;
    }

    #[tokio::test]
    async fn the_recording_browser_meets_the_browser_contract() {
        let (browser, mut urls) = RecordingBrowser::new();
        browser_suite(&browser).await;
        assert_eq!(urls.recv().await.as_deref(), Some("http://127.0.0.1:9/authorize?state=s"));
    }

    #[test]
    fn the_memory_keychain_meets_the_keychain_contract() {
        keychain_suite(&MemoryKeychain::default());
        assert_eq!(format!("{:?}", MemoryKeychain::new()), "{}");
    }
}
