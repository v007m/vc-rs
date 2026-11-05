use serde_json::json;
use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CachedCleanup, CachedValues};

/// Builds an empty store.
pub type MakeCacheStore<'a> = dyn FnMut() -> Box<dyn CacheStore> + 'a;

/// The contract every [`CacheStore`] meets (D112). `make` builds an empty store.
pub fn cache_store_suite(make: &mut MakeCacheStore<'_>) {
    gives_back_live_entries_whole(make);
    hides_and_takes_expired_entries(make);
    lists_and_clears_by_label(make);
}

/// An entry for `label`, made at `created_ms` and kept for 1000 ms.
fn entry(seed: u8, label: &str, created_ms: u64) -> CacheEntry {
    CacheEntry {
        key: CacheKey::new([seed; 32]),
        listing: CacheListing { label: label.into(), profiles: "dev".into(), created_ms, expires_ms: created_ms + 1000 },
        saved: values(&json!({"userId": seed, "token": "tok-secret"})),
        cleanups: vec![CachedCleanup { api: label.into(), vars: values(&json!({"userId": seed})) }],
    }
}

fn values(value: &serde_json::Value) -> CachedValues {
    value.as_object().cloned().unwrap_or_default()
}

fn put(store: &dyn CacheStore, entry: CacheEntry) {
    assert_eq!(store.put(entry), Ok(()), "put failed");
}

fn labels(entries: &[CacheEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.listing.label.as_str()).collect()
}

fn gives_back_live_entries_whole(make: &mut MakeCacheStore<'_>) {
    let store = make();
    let key = CacheKey::new([1; 32]);
    assert_eq!(store.get(&key, 0), Ok(None), "an empty store has nothing");
    put(store.as_ref(), entry(1, "users#create", 100));
    assert_eq!(store.get(&key, 500), Ok(Some(entry(1, "users#create", 100))), "an entry comes back as it was put");
    let mut newer = entry(1, "users#create", 200);
    newer.saved = values(&json!({"userId": 9}));
    put(store.as_ref(), newer.clone());
    assert_eq!(store.get(&key, 500), Ok(Some(newer)), "a put replaces the entry under its key");
}

fn hides_and_takes_expired_entries(make: &mut MakeCacheStore<'_>) {
    let store = make();
    put(store.as_ref(), entry(1, "users#create", 0));
    put(store.as_ref(), entry(2, "orders#create", 500));
    assert_eq!(store.get(&CacheKey::new([1; 32]), 1000), Ok(None), "an entry is gone once it expires");
    let expired = store.take_expired(1000);
    assert_eq!(expired.as_ref().map(|taken| labels(taken)), Ok(vec!["users#create"]));
    assert_eq!(expired.map(|mut taken| taken.pop().map(|entry| entry.cleanups)), Ok(Some(entry(1, "users#create", 0).cleanups)));
    assert_eq!(store.take_expired(1000), Ok(Vec::new()), "taking removes them");
    assert_eq!(store.list().map(|listed| listed.len()), Ok(1), "live entries stay");
}

fn lists_and_clears_by_label(make: &mut MakeCacheStore<'_>) {
    let store = make();
    put(store.as_ref(), entry(3, "users#create", 300));
    put(store.as_ref(), entry(1, "orders#create", 100));
    put(store.as_ref(), entry(2, "users#create", 200));
    let listed = store.list().map(|listed| listed.into_iter().map(|listing| listing.created_ms).collect::<Vec<_>>());
    assert_eq!(listed, Ok(vec![100, 200, 300]), "listed oldest first");
    let cleared = store.clear(Some("users#create"));
    assert_eq!(cleared.map(|taken| taken.iter().map(|entry| entry.listing.created_ms).collect::<Vec<_>>()), Ok(vec![200, 300]));
    assert_eq!(store.clear(None).as_ref().map(|taken| labels(taken)), Ok(vec!["orders#create"]));
    assert_eq!(store.list(), Ok(Vec::new()), "clearing everything empties the store");
}
