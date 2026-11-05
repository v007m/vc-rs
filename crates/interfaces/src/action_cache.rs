//! The cross-run cache (`.vibranium/cache.db`): what a `cache:`d action saved, kept until its
//! `for` runs out, with the cleanups that undo it.
//!
//! A store keeps entries encrypted at rest; this port hands it plain values and never a key.

use std::fmt;

/// Values an action saved, by name, as JSON.
pub type CachedValues = serde_json::Map<String, serde_json::Value>;

/// Keeps cached action results between runs. `vc-testkit` has a memory fake, and every
/// implementation runs `cache_store_suite` (D112).
pub trait CacheStore: Send + Sync {
    /// The live entry under `key` at `now_ms`; `None` when there is none, it has expired, or
    /// it can no longer be read (another key, a damaged row).
    ///
    /// # Errors
    ///
    /// [`CacheStoreError`] when the store cannot be read.
    fn get(&self, key: &CacheKey, now_ms: u64) -> Result<Option<CacheEntry>, CacheStoreError>;

    /// Stores `entry`, replacing any entry under its key.
    ///
    /// # Errors
    ///
    /// [`CacheStoreError`] when the store cannot be written.
    fn put(&self, entry: CacheEntry) -> Result<(), CacheStoreError>;

    /// Removes and returns every entry expired at `now_ms`, oldest first, so their cleanups
    /// can run.
    ///
    /// # Errors
    ///
    /// [`CacheStoreError`] when the store cannot be read or written.
    fn take_expired(&self, now_ms: u64) -> Result<Vec<CacheEntry>, CacheStoreError>;

    /// Every entry's listing, oldest first; values are left out.
    ///
    /// # Errors
    ///
    /// [`CacheStoreError`] when the store cannot be read.
    fn list(&self) -> Result<Vec<CacheListing>, CacheStoreError>;

    /// Removes and returns the entries whose label is `label`, or every entry for `None`, so
    /// their cleanups can run.
    ///
    /// # Errors
    ///
    /// [`CacheStoreError`] when the store cannot be read or written.
    fn clear(&self, label: Option<&str>) -> Result<Vec<CacheEntry>, CacheStoreError>;
}

/// An entry's key: blake3 of the api or test ID, the canonical resolved vars and the profiles.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    /// The key with these bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Its bytes.
    #[must_use]
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let head = self.0.iter().take(4).map(|byte| format!("{byte:02x}")).collect::<String>();
        write!(f, "CacheKey({head}…)")
    }
}

/// A cached result.
#[derive(Clone, PartialEq)]
#[expect(clippy::exhaustive_structs, reason = "cache entry built and read across crates; every user is in this repo (D160)")]
pub struct CacheEntry {
    /// Its key.
    pub key: CacheKey,
    /// What it caches, readable: the api or test ID and the profiles (`users#create dev`).
    pub listing: CacheListing,
    /// What the action saved.
    pub saved: CachedValues,
    /// The cleanups its call registered, run when it expires or is cleared, newest last.
    pub cleanups: Vec<CachedCleanup>,
}

/// Values may hold secrets, so `Debug` names them only.
impl fmt::Debug for CacheEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheEntry")
            .field("key", &self.key)
            .field("listing", &self.listing)
            .field("saved", &self.saved.keys().collect::<Vec<_>>())
            .field("cleanups", &self.cleanups.len())
            .finish()
    }
}

/// What `vc cache list` shows of an entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "cache entry built and read across crates; every user is in this repo (D160)")]
pub struct CacheListing {
    /// The api or test ID it caches: `users#create`.
    pub label: String,
    /// The profiles it was made under, comma-separated.
    pub profiles: String,
    /// When it was made, in Unix milliseconds.
    pub created_ms: u64,
    /// When it expires, in Unix milliseconds.
    pub expires_ms: u64,
}

/// A cleanup kept with an entry: the api whose `cleanup` it is, by full ID, since action IDs
/// change between compiles, and the vars it runs with.
#[derive(Clone, PartialEq)]
#[expect(clippy::exhaustive_structs, reason = "cache entry built and read across crates; every user is in this repo (D160)")]
pub struct CachedCleanup {
    /// The full ID of the api whose `cleanup` runs: `users#create`.
    pub api: String,
    /// Its vars.
    pub vars: CachedValues,
}

/// Vars may hold secrets, so `Debug` names them only.
impl fmt::Debug for CachedCleanup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachedCleanup")
            .field("api", &self.api)
            .field("vars", &self.vars.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Why the cache could not be used. A cache failure never fails a test: the action runs
/// uncached.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CacheStoreError {
    /// The store could not be opened or created.
    #[error("cannot open the cache at {location}: {reason}")]
    Open {
        /// Where the store is.
        location: String,
        /// What went wrong.
        reason: String,
    },
    /// A read or write failed.
    #[error("the cache failed: {reason}")]
    Failed {
        /// What went wrong.
        reason: String,
    },
    /// The store's task is no longer running.
    #[error("the cache stopped")]
    Stopped,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CacheEntry, CacheKey, CacheListing, CachedCleanup};

    #[test]
    fn debug_output_never_shows_values() {
        let saved = json!({"token": "Sx7Q-secret"}).as_object().cloned().unwrap_or_default();
        let cleanup = CachedCleanup { api: "users#create".into(), vars: saved.clone() };
        let entry = CacheEntry {
            key: CacheKey::new([0xab; 32]),
            listing: CacheListing { label: "users#create".into(), ..CacheListing::default() },
            saved,
            cleanups: vec![cleanup],
        };
        let shown = format!("{entry:?} {:?}", entry.cleanups);
        assert!(!shown.contains("Sx7Q"), "{shown}");
        assert!(shown.contains("token") && shown.contains("users#create") && shown.contains("abababab"), "{shown}");
    }

    #[test]
    fn keys_give_back_their_bytes() {
        assert_eq!(CacheKey::new([7; 32]).bytes(), &[7; 32]);
    }
}
