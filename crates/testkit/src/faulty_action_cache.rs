use std::sync::Arc;

use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CacheStoreError, TakenEntries};

/// What a [`FaultyCacheStore`] gets wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CacheFault {
    /// `get` fails, as a store that cannot be read.
    Reads,
    /// `put` and `remove` fail, as a store that cannot be written.
    Writes,
    /// Each take (`take_expired`, `clear`) also removes this many rows that do not open.
    Unopened(usize),
}

/// Wraps a cache store and fails as its faults say (a decorator); with none it is the store.
#[derive(Clone)]
pub struct FaultyCacheStore {
    inner: Arc<dyn CacheStore>,
    faults: Arc<[CacheFault]>,
}

impl std::fmt::Debug for FaultyCacheStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaultyCacheStore").field("faults", &self.faults).finish_non_exhaustive()
    }
}

impl FaultyCacheStore {
    /// `inner`, with `faults`.
    #[must_use]
    pub fn new(inner: Arc<dyn CacheStore>, faults: Vec<CacheFault>) -> Self {
        Self { inner, faults: faults.into() }
    }

    fn fails(&self, fault: CacheFault) -> Result<(), CacheStoreError> {
        if self.faults.contains(&fault) {
            return Err(CacheStoreError::Failed { reason: "disk I/O error".to_owned() });
        }
        Ok(())
    }

    /// `taken`, with the unopened rows the faults add.
    fn unopened(&self, taken: TakenEntries) -> TakenEntries {
        let more = self.faults.iter().map(|fault| if let CacheFault::Unopened(count) = fault { *count } else { 0 });
        TakenEntries { unopened: taken.unopened + more.sum::<usize>(), ..taken }
    }
}

impl CacheStore for FaultyCacheStore {
    fn get(&self, key: &CacheKey, now_ms: u64) -> Result<Option<CacheEntry>, CacheStoreError> {
        self.fails(CacheFault::Reads)?;
        self.inner.get(key, now_ms)
    }

    fn put(&self, entry: CacheEntry) -> Result<Option<CacheEntry>, CacheStoreError> {
        self.fails(CacheFault::Writes)?;
        self.inner.put(entry)
    }

    fn remove(&self, key: &CacheKey) -> Result<(), CacheStoreError> {
        self.fails(CacheFault::Writes)?;
        self.inner.remove(key)
    }

    fn take_expired(&self, now_ms: u64) -> Result<TakenEntries, CacheStoreError> {
        self.inner.take_expired(now_ms).map(|taken| self.unopened(taken))
    }

    fn list(&self) -> Result<Vec<CacheListing>, CacheStoreError> {
        self.inner.list()
    }

    fn clear(&self, label: Option<&str>) -> Result<TakenEntries, CacheStoreError> {
        self.inner.clear(label).map(|taken| self.unopened(taken))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vc_interfaces::{CacheKey, CacheStore as _, CacheStoreError, TakenEntries};

    use super::{CacheFault, FaultyCacheStore};
    use crate::{MemoryCacheStore, cache_store_suite};

    #[test]
    fn passes_the_cache_store_suite_without_faults() {
        cache_store_suite(&mut || Box::new(FaultyCacheStore::new(Arc::new(MemoryCacheStore::new()), Vec::new())));
    }

    #[test]
    fn fails_reads_and_writes_and_counts_unopened_rows_as_asked() {
        let faults = vec![CacheFault::Reads, CacheFault::Writes, CacheFault::Unopened(2)];
        let store = FaultyCacheStore::new(Arc::new(MemoryCacheStore::new()), faults);
        let failed = Err(CacheStoreError::Failed { reason: "disk I/O error".to_owned() });
        assert_eq!(store.get(&CacheKey::new([1; 32]), 0), failed.clone().map(|()| None));
        assert_eq!(store.remove(&CacheKey::new([1; 32])), failed);
        assert_eq!(store.take_expired(0), Ok(TakenEntries { entries: Vec::new(), unopened: 2 }));
        assert_eq!(store.clear(None).map(|taken| taken.unopened), Ok(2));
    }
}
