use std::collections::BTreeMap;
use std::sync::mpsc::{self, Sender};

use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CacheStoreError, TakenEntries};

/// The entries, by key.
type Entries = BTreeMap<CacheKey, CacheEntry>;

/// A job the store's thread runs against its entries.
type Job = Box<dyn FnOnce(&mut Entries) + Send>;

/// A cross-run cache held in memory.
///
/// A small thread owns the entries, so the store is shared without a lock. Clones talk to the
/// same thread, so a test can keep one to look at what the executor cached.
#[derive(Clone, Debug)]
pub struct MemoryCacheStore {
    jobs: Sender<Job>,
}

impl MemoryCacheStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            let mut entries = Entries::new();
            for job in inbox {
                job(&mut entries);
            }
        });
        Self { jobs }
    }

    fn ask<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Entries) -> T + Send + 'static,
    ) -> Result<T, CacheStoreError> {
        let (reply, answer) = mpsc::channel();
        let sent = self.jobs.send(Box::new(move |entries| drop(reply.send(job(entries)))));
        sent.ok().and_then(|()| answer.recv().ok()).ok_or(CacheStoreError::Stopped)
    }
}

impl Default for MemoryCacheStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheStore for MemoryCacheStore {
    fn get(&self, key: &CacheKey, now_ms: u64) -> Result<Option<CacheEntry>, CacheStoreError> {
        let key = *key;
        self.ask(move |entries| entries.get(&key).filter(|entry| entry.listing.expires_ms > now_ms).cloned())
    }

    fn put(&self, entry: CacheEntry) -> Result<Option<CacheEntry>, CacheStoreError> {
        self.ask(move |entries| {
            let made_ms = entry.listing.created_ms;
            let replaced = entries.insert(entry.key, entry)?;
            let key = vacant(entries, replaced.key);
            let expires_ms = replaced.listing.expires_ms.min(made_ms);
            let buried = CacheEntry { key, listing: CacheListing { expires_ms, ..replaced.listing }, ..replaced };
            entries.insert(key, buried.clone());
            Some(buried)
        })
    }

    fn remove(&self, key: &CacheKey) -> Result<(), CacheStoreError> {
        let key = *key;
        self.ask(move |entries| drop(entries.remove(&key)))
    }

    fn take_expired(&self, now_ms: u64) -> Result<TakenEntries, CacheStoreError> {
        self.ask(move |entries| take(entries, |entry| entry.listing.expires_ms <= now_ms))
    }

    fn list(&self) -> Result<Vec<CacheListing>, CacheStoreError> {
        self.ask(|entries| {
            oldest_first(entries.values().cloned().collect()).into_iter().map(|entry| entry.listing).collect()
        })
    }

    fn clear(&self, label: Option<&str>) -> Result<TakenEntries, CacheStoreError> {
        let label = label.map(str::to_owned);
        self.ask(move |entries| take(entries, |entry| label.as_ref().is_none_or(|label| *label == entry.listing.label)))
    }
}

/// The first key that holds no entry among `near` with its last bytes flipped by 1, 2, 3 and
/// on: a tombstone's key.
fn vacant(entries: &Entries, near: CacheKey) -> CacheKey {
    let flipped = |step: u64| {
        let mut bytes = *near.bytes();
        for (byte, flip) in bytes.iter_mut().rev().zip(step.to_le_bytes()) {
            *byte ^= flip;
        }
        CacheKey::new(bytes)
    };
    (1..=u64::MAX).map(flipped).find(|key| !entries.contains_key(key)).unwrap_or(near)
}

/// Removes the entries `matches` picks and gives them back, oldest first; every entry here
/// opens, so none is counted as unopened.
fn take(entries: &mut Entries, matches: impl Fn(&CacheEntry) -> bool) -> TakenEntries {
    let (taken, kept): (Vec<CacheEntry>, Vec<CacheEntry>) =
        std::mem::take(entries).into_values().partition(|entry| matches(entry));
    *entries = kept.into_iter().map(|entry| (entry.key, entry)).collect();
    TakenEntries { entries: oldest_first(taken), unopened: 0 }
}

fn oldest_first(mut entries: Vec<CacheEntry>) -> Vec<CacheEntry> {
    entries.sort_by_key(|entry| (entry.listing.created_ms, entry.key));
    entries
}

#[cfg(test)]
mod tests {
    use super::MemoryCacheStore;
    use crate::cache_store_suite;

    #[test]
    fn passes_the_cache_store_suite() {
        cache_store_suite(&mut || Box::new(MemoryCacheStore::new()));
    }
}
