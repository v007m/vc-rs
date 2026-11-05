use std::path::Path;

use rusqlite::Connection;
use serde_json::json;
use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CachedCleanup};
use vc_testkit::cache_store_suite;

use super::SqliteCache;
use crate::crypt::CacheSecret;

const SECRET: &str = "tok-Sx7Q-9vLk";

fn entry(seed: u8) -> CacheEntry {
    let saved = json!({"token": SECRET, "userId": seed}).as_object().cloned().unwrap_or_default();
    CacheEntry {
        key: CacheKey::new([seed; 32]),
        listing: CacheListing { label: "users#create".into(), profiles: "dev".into(), created_ms: 10, expires_ms: 1000 },
        cleanups: vec![CachedCleanup { api: "users#create".into(), vars: saved.clone() }],
        saved,
    }
}

fn open(path: &Path, key: u8) -> SqliteCache {
    SqliteCache::open(path, &CacheSecret::new([key; 32])).unwrap()
}

#[test]
fn passes_the_cache_store_suite() {
    let dir = tempfile::tempdir().unwrap();
    let mut made = 0;
    cache_store_suite(&mut || {
        made += 1;
        Box::new(open(&dir.path().join(format!("cache{made}.db")), 1))
    });
}

#[test]
fn keeps_values_sealed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".vibranium/cache.db");
    open(&path, 1).put(entry(1)).unwrap();
    let connection = Connection::open(&path).unwrap();
    let (saved, cleanups): (Vec<u8>, Vec<u8>) =
        connection.query_row("SELECT saved, cleanups FROM entries", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    for sealed in [saved, cleanups] {
        assert!(!sealed.windows(SECRET.len()).any(|window| window == SECRET.as_bytes()), "a value is in the clear");
    }
    assert_eq!(open(&path, 1).get(&CacheKey::new([1; 32]), 500).unwrap(), Some(entry(1)), "it opens again");
}

#[test]
fn a_row_that_no_longer_opens_is_a_miss_and_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    open(&path, 1).put(entry(1)).unwrap();
    open(&path, 1).put(entry(2)).unwrap();
    let other_key = open(&path, 2);
    assert_eq!(other_key.get(&CacheKey::new([1; 32]), 500).unwrap(), None, "another key misses");
    assert_eq!(other_key.list().unwrap().len(), 1, "and drops the row");
    let connection = Connection::open(&path).unwrap();
    connection.execute("UPDATE entries SET saved = saved || x'00'", []).unwrap();
    let cache = open(&path, 1);
    assert_eq!(cache.clear(None).unwrap(), Vec::new(), "a changed row is cleared without its cleanups");
    assert_eq!(cache.list().unwrap(), Vec::new());
}

#[test]
fn a_cache_that_cannot_be_created_says_where() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("file");
    std::fs::write(&blocker, "").unwrap();
    let error = SqliteCache::open(&blocker.join("cache.db"), &CacheSecret::new([1; 32])).unwrap_err();
    assert!(error.to_string().contains("cannot open the cache at"), "{error}");
}
