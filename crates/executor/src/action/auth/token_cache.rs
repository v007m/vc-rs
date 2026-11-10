//! The run's token cache: one write-once slot per auth key, so requests that need the same
//! credentials at the same moment wait for one fetch (D113, D168).
#![expect(
    clippy::disallowed_types,
    reason = "the token cache is one of the three places shared mutable state is allowed; the lock is never held across an await"
)]

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::hash::{BuildHasher, RandomState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::OnceCell;
use tokio::time::Instant;
use vc_interfaces::{AuthError, Credentials};

/// What a slot holds: the credentials (or why there are none) and when they go stale.
#[derive(Clone)]
struct Cached {
    result: Result<Credentials, AuthError>,
    stale_at: Option<Instant>,
}

impl Cached {
    fn stale(&self) -> bool {
        self.stale_at.is_some_and(|at| Instant::now() >= at)
    }
}

type Slot = Arc<OnceCell<Cached>>;

/// The cache key of one auth with its resolved vars and settings: a BLAKE3 hash keyed with a
/// random per-run key, so no plain hash of a secret value ever exists (D168).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuthKey([u8; 32]);

impl fmt::Debug for AuthKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthKey(..)")
    }
}

/// The slot one call last took its credentials from, so that a `refreshOn` response drops
/// that slot and never one another request has filled again since (compare-and-invalidate).
pub struct Lease {
    key: AuthKey,
    held: Mutex<Option<Slot>>,
}

impl fmt::Debug for Lease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lease").finish_non_exhaustive()
    }
}

impl Lease {
    /// A lease on the credentials for `key` that holds no slot yet.
    #[must_use]
    pub fn new(key: AuthKey) -> Self {
        Self { key, held: Mutex::default() }
    }

    fn hold(&self, slot: Slot) {
        *self.held.lock().unwrap_or_else(PoisonError::into_inner) = Some(slot);
    }

    fn held(&self) -> Option<Slot> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

/// The run's credentials by [`AuthKey`]. The map's lock is only ever held to find or replace a
/// slot, never across an `await`; each slot is filled once by the first request that needs it.
pub struct TokenCache {
    key: [u8; 32],
    slots: Mutex<HashMap<AuthKey, Slot>>,
}

impl fmt::Debug for TokenCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenCache").finish_non_exhaustive()
    }
}

impl Default for TokenCache {
    fn default() -> Self {
        Self::new(random_key())
    }
}

impl TokenCache {
    /// A cache hashing with `key`.
    #[must_use]
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, slots: Mutex::default() }
    }

    /// The key of auth `id` with its resolved `vars` and `settings`.
    #[must_use]
    pub fn key(&self, id: &str, vars: &Value, settings: &Value) -> AuthKey {
        let mut hasher = blake3::Hasher::new_keyed(&self.key);
        for part in [Value::from(id), vars.clone(), settings.clone()] {
            let bytes = part.to_string();
            hasher.update(&u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
            hasher.update(bytes.as_bytes());
        }
        AuthKey(*hasher.finalize().as_bytes())
    }

    /// The credentials for the lease's key, fetched once with `fetch` and again once they are within
    /// `refresh_before`, capped at half their lifetime, of expiring (D193). The request that
    /// fetched them always uses them, however short their life. `lease` holds the slot they
    /// came from.
    ///
    /// # Errors
    ///
    /// The error `fetch` gave. A refusal, like any failure that will not pass, is kept for the
    /// run, so requests do not each retry a login and lock the account. A failure that may pass
    /// ([`AuthError::may_pass`]) is shared with the requests already waiting on that fetch but
    /// not kept: the next request fetches again.
    pub async fn get<F, Fut>(&self, lease: &Lease, refresh_before: Duration, fetch: F) -> Result<Credentials, AuthError>
    where
        F: Fn() -> Fut + Sync,
        Fut: Future<Output = Result<Credentials, AuthError>> + Send,
    {
        let fetched = AtomicBool::new(false);
        let fill = || async {
            fetched.store(true, Ordering::Relaxed);
            cached(fetch().await, refresh_before)
        };
        let mut slot = self.slot(lease.key);
        if slot.get_or_init(fill).await.stale() && !fetched.load(Ordering::Relaxed) {
            self.replace(lease.key, &slot);
            slot = self.slot(lease.key);
        }
        let result = slot.get_or_init(fill).await.result.clone();
        if result.as_ref().is_err_and(AuthError::may_pass) {
            self.replace(lease.key, &slot);
        }
        lease.hold(slot);
        result
    }

    /// Forgets the credentials `lease` last took, so the next request fetches them again,
    /// unless another request has already fetched them again since.
    pub fn invalidate(&self, lease: &Lease) {
        if let Some(held) = lease.held() {
            self.replace(lease.key, &held);
        }
    }

    fn slot(&self, key: AuthKey) -> Slot {
        Arc::clone(self.lock().entry(key).or_default())
    }

    /// Drops `stale` unless another request has already replaced it.
    fn replace(&self, key: AuthKey, stale: &Slot) {
        let mut slots = self.lock();
        if slots.get(&key).is_some_and(|current| Arc::ptr_eq(current, stale)) {
            slots.remove(&key);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<AuthKey, Slot>> {
        self.slots.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn cached(result: Result<Credentials, AuthError>, refresh_before: Duration) -> Cached {
    let lifetime = result.as_ref().ok().and_then(|credentials| credentials.expires_in);
    let stale_at = lifetime.map(|lifetime| Instant::now() + lifetime.saturating_sub(refresh_before.min(lifetime / 2)));
    Cached { result, stale_at }
}

/// A key from the process's random hasher seeds: unpredictable, and never written anywhere.
fn random_key() -> [u8; 32] {
    let state = RandomState::new();
    let mut key = [0; 32];
    for (index, chunk) in key.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        chunk.copy_from_slice(&state.hash_one(index).to_le_bytes());
    }
    key
}

#[cfg(test)]
#[path = "token_cache_tests.rs"]
mod tests;
