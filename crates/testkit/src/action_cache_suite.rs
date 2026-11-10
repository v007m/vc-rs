use serde_json::json;
use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CachedCleanup, CachedValues};

/// Builds an empty store.
pub type MakeCacheStore<'a> = dyn FnMut() -> Box<dyn CacheStore> + 'a;

/// The contract every [`CacheStore`] meets (D112). `make` builds an empty store.
pub fn cache_store_suite(make: &mut MakeCacheStore<'_>) {
    gives_back_live_entries_whole(make);
    keeps_a_replaced_entry_as_an_expired_tombstone(make);
    removes_a_tombstone_or_takes_it_as_expired(make);
    hides_and_takes_expired_entries(make);
    lists_and_clears_by_label(make);
}

/// An entry for `label`, made at `created_ms` and kept for 1000 ms.
fn entry(seed: u8, label: &str, created_ms: u64) -> CacheEntry {
    CacheEntry {
        key: CacheKey::new([seed; 32]),
        listing: CacheListing {
            label: label.into(),
            profiles: "dev".into(),
            created_ms,
            expires_ms: created_ms + 1000,
        },
        saved: values(&json!({"userId": seed, "token": "tok-secret"})),
        cleanups: vec![CachedCleanup { api: label.into(), vars: values(&json!({"userId": seed})) }],
    }
}

fn values(value: &serde_json::Value) -> CachedValues {
    value.as_object().cloned().unwrap_or_default()
}

/// Puts `entry` and gives back the tombstone of the entry it replaced.
fn put(store: &dyn CacheStore, entry: CacheEntry) -> Option<CacheEntry> {
    let put = store.put(entry);
    assert!(put.is_ok(), "put failed: {put:?}");
    put.ok().flatten()
}

fn labels(entries: &[CacheEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.listing.label.as_str()).collect()
}

fn gives_back_live_entries_whole(make: &mut MakeCacheStore<'_>) {
    let store = make();
    let key = CacheKey::new([1; 32]);
    assert_eq!(store.get(&key, 0), Ok(None), "an empty store has nothing");
    assert_eq!(put(store.as_ref(), entry(1, "users#create", 100)), None, "a first put replaces nothing");
    assert_eq!(store.get(&key, 500), Ok(Some(entry(1, "users#create", 100))), "an entry comes back as it was put");
    let mut newer = entry(1, "users#create", 200);
    newer.saved = values(&json!({"userId": 9}));
    let replaced = put(store.as_ref(), newer.clone()).map(|buried| (buried.saved, buried.cleanups));
    let first = entry(1, "users#create", 100);
    assert_eq!(replaced, Some((first.saved, first.cleanups)), "a put gives back the entry it replaced");
    assert_eq!(store.get(&key, 500), Ok(Some(newer)), "a put replaces the entry under its key");
    let mut later = entry(1, "users#create", 5000);
    later.saved = values(&json!({"userId": 7}));
    let expired = put(store.as_ref(), later).map(|entry| entry.listing.created_ms);
    assert_eq!(expired, Some(200), "an expired entry put over comes back too");
}

/// A store holding `users#create` made at 100 and replaced at 200, and the tombstone the
/// replacing put gave back.
fn replaced(make: &mut MakeCacheStore<'_>) -> (Box<dyn CacheStore>, Option<CacheEntry>) {
    let store = make();
    put(store.as_ref(), entry(1, "users#create", 100));
    let buried = put(store.as_ref(), entry(1, "users#create", 200));
    assert!(buried.is_some(), "a put over an entry gives back its tombstone");
    (store, buried)
}

fn keeps_a_replaced_entry_as_an_expired_tombstone(make: &mut MakeCacheStore<'_>) {
    let (store, buried) = replaced(make);
    let Some(buried) = buried else { return };
    let key = CacheKey::new([1; 32]);
    let first = entry(1, "users#create", 100);
    assert_ne!(buried.key, key, "a tombstone has a key of its own");
    assert_eq!((&buried.saved, &buried.cleanups), (&first.saved, &first.cleanups), "and the replaced entry's values");
    let listing = CacheListing { expires_ms: 200, ..first.listing };
    assert_eq!(buried.listing, listing, "it expired when the fresh entry was made");
    assert_eq!(store.get(&key, 500), Ok(Some(entry(1, "users#create", 200))), "the fresh entry is live");
    let again = put(store.as_ref(), entry(1, "users#create", 300)).map(|second| second.key);
    assert!(again.is_some_and(|second| second != buried.key), "every tombstone has its own key");
    assert_eq!(store.list().map(|listed| listed.len()), Ok(3), "tombstones are listed until removed");
}

fn removes_a_tombstone_or_takes_it_as_expired(make: &mut MakeCacheStore<'_>) {
    let (store, buried) = replaced(make);
    let Some(buried) = buried else { return };
    put(store.as_ref(), entry(1, "users#create", 300));
    assert_eq!(store.remove(&buried.key), Ok(()));
    assert_eq!(store.remove(&buried.key), Ok(()), "removing nothing is no error");
    let expired = store.take_expired(1000).map(|taken| taken.iter().map(|e| e.listing.created_ms).collect::<Vec<_>>());
    assert_eq!(expired, Ok(vec![200]), "a tombstone left in place is taken as expired");
    let live = store.get(&CacheKey::new([1; 32]), 500);
    assert_eq!(live, Ok(Some(entry(1, "users#create", 300))), "the live entry stays");
}

fn hides_and_takes_expired_entries(make: &mut MakeCacheStore<'_>) {
    let store = make();
    put(store.as_ref(), entry(1, "users#create", 0));
    put(store.as_ref(), entry(2, "orders#create", 500));
    assert_eq!(store.get(&CacheKey::new([1; 32]), 1000), Ok(None), "an entry is gone once it expires");
    let expired = store.take_expired(1000);
    assert_eq!(expired.as_ref().map(|taken| labels(taken)), Ok(vec!["users#create"]));
    assert_eq!(
        expired.map(|mut taken| taken.pop().map(|entry| entry.cleanups)),
        Ok(Some(entry(1, "users#create", 0).cleanups))
    );
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
    assert_eq!(
        cleared.map(|taken| taken.iter().map(|entry| entry.listing.created_ms).collect::<Vec<_>>()),
        Ok(vec![200, 300])
    );
    assert_eq!(store.clear(None).as_ref().map(|taken| labels(taken)), Ok(vec!["orders#create"]));
    assert_eq!(store.list(), Ok(Vec::new()), "clearing everything empties the store");
}
