use std::sync::Arc;

use serde_json::json;
use tokio::sync::mpsc;
use vc_executor::{CacheEvent, RunInputs};
use vc_interfaces::{
    CacheEntry, CacheKey, CacheListing, CacheStore, CachedCleanup, CachedValues, Keychain, KeychainError, Secret,
    SourceProvider,
};
use vc_testkit::{
    CacheFault, FakeVcs, FaultyCacheStore, MemoryCacheStore, MemoryEnvironment, MemoryKeychain, MemorySources,
};

use super::{ACCOUNT, Buried, SERVICE, Sweep};
use crate::plugin_hosts::Hosts;
use crate::{Engine, interrupts, ports, sample};

const KEY: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A keychain that cannot be reached, as when it is locked.
struct LockedKeychain;

impl Keychain for LockedKeychain {
    fn get(&self, _: &str, _: &str) -> Result<Option<Secret>, KeychainError> {
        Err(KeychainError::new("the keychain is locked"))
    }

    fn set(&self, _: &str, _: &str, _: &Secret) -> Result<(), KeychainError> {
        Err(KeychainError::new("the keychain is locked"))
    }

    fn delete(&self, _: &str, _: &str) -> Result<(), KeychainError> {
        Ok(())
    }
}

fn keyed(keychain: Arc<dyn Keychain>) -> Engine {
    sample::builder(FakeVcs::default()).keychain(keychain).build().unwrap()
}

fn stored(store: &Arc<MemoryCacheStore>) -> Engine {
    let store: Arc<dyn CacheStore> = Arc::clone(store) as _;
    sample::builder(FakeVcs::default()).keychain(Arc::new(LockedKeychain)).cache(store).build().unwrap()
}

/// A project whose `beforeRun` is cached, so a run opens `store`.
fn caching(store: Arc<dyn CacheStore>) -> Engine {
    let config = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n\
                  beforeRun:\n  - id: admin\n    call: users#list\n    cache: {for: 1h}\n";
    let api = "baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n";
    let files = [("vibranium.config.yaml", config.as_bytes()), ("services/users/users.api.yaml", api.as_bytes())];
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&files));
    sample::builder(FakeVcs::default())
        .sources(sources)
        .keychain(Arc::new(LockedKeychain))
        .cache(store)
        .build()
        .unwrap()
}

/// `store` with one entry, and `unopened` rows that do not open.
fn unopened(store: &Arc<MemoryCacheStore>, unopened: usize) -> Arc<dyn CacheStore> {
    Arc::new(FaultyCacheStore::new(Arc::clone(store) as _, vec![CacheFault::Unopened(unopened)]))
}

fn entry(byte: u8, label: &str, profiles: &str, expires_ms: u64) -> CacheEntry {
    let listing = CacheListing { label: label.to_owned(), profiles: profiles.to_owned(), created_ms: 0, expires_ms };
    let saved = CachedValues::from_iter([("userId".to_owned(), json!(7))]);
    CacheEntry { key: CacheKey::new([byte; 32]), listing, saved, cleanups: Vec::new() }
}

#[test]
fn the_key_comes_from_the_variable_before_the_keychain() {
    let engine = keyed(Arc::new(LockedKeychain));
    let given = engine.cache_secret(&MemoryEnvironment::new(&[("VIB_CACHE_KEY", KEY)])).unwrap();
    assert_eq!(given.to_hex().as_str(), KEY);
    let error = engine.cache_secret(&MemoryEnvironment::new(&[("VIB_CACHE_KEY", "short")])).unwrap_err();
    assert!(error.to_string().contains("V5113") && error.to_string().contains("64 hex digits"), "{error}");
}

#[test]
fn the_keychain_key_is_made_once_and_then_reused() {
    let keychain = Arc::new(MemoryKeychain::new());
    let engine = keyed(Arc::clone(&keychain) as Arc<dyn Keychain>);
    let made = engine.cache_secret(&MemoryEnvironment::new(&[])).unwrap();
    let kept = keychain.get(SERVICE, ACCOUNT).unwrap().unwrap();
    assert_eq!(kept.expose(), made.to_hex().as_str());
    let again = engine.cache_secret(&MemoryEnvironment::new(&[])).unwrap();
    assert_eq!(again.to_hex(), made.to_hex());
}

#[test]
fn a_keychain_that_cannot_be_used_is_v5113() {
    let error = keyed(Arc::new(LockedKeychain)).cache_secret(&MemoryEnvironment::new(&[])).unwrap_err();
    let text = error.to_string();
    assert!(text.starts_with("cache: V5113") && text.contains("the keychain is locked"), "{text}");
    assert!(error.help().unwrap_or_default().contains("VIB_CACHE_KEY"));
    assert!(super::off(&error).contains("`cache:` actions run every time this run"));
}

#[test]
fn lists_the_entries_without_their_values() {
    let store = Arc::new(MemoryCacheStore::new());
    store.put(entry(1, "users#create", "dev,eu", 0)).unwrap();
    store.put(entry(2, "users.flows#signsUp", "", 86_400_000)).unwrap();
    let report = stored(&store).cache_list().unwrap();
    let text = report.text();
    assert!(text.contains("users#create  expired 1970-01-01 00:00Z  profile dev,eu\n"), "{text}");
    assert!(text.contains("users.flows#signsUp  expires 1970-01-02 00:00Z\n"), "{text}");
    let listed = report.json();
    assert_eq!(listed[0]["profiles"], json!(["dev", "eu"]));
    assert_eq!(listed[1]["profiles"], json!([]));
    assert!(!listed.to_string().contains("userId"), "values are never listed: {listed}");
    let empty = stored(&Arc::new(MemoryCacheStore::new())).cache_list().unwrap();
    assert_eq!(empty.text(), "the cache is empty\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_reports_a_cleanup_whose_api_has_none() {
    let store = Arc::new(MemoryCacheStore::new());
    let mut made = entry(1, "users#create", "dev", 1);
    made.cleanups.push(CachedCleanup { api: "users#gone".to_owned(), vars: CachedValues::new() });
    store.put(made).unwrap();
    store.put(entry(2, "users#list", "", 1)).unwrap();
    let engine = stored(&store);
    assert_eq!(engine.cache_clear(Some("orders#create")).await.unwrap(), (0, Vec::new()));
    let (count, notices) = engine.cache_clear(Some("users#create")).await.unwrap();
    assert_eq!(count, 1);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].contains("users#create") && notices[0].contains("no longer has a `cleanup`"), "{notices:?}");
    assert_eq!(engine.cache_clear(None).await.unwrap(), (1, Vec::new()));
    assert_eq!(engine.cache_list().unwrap().text(), "the cache is empty\n");
}

#[test]
fn a_store_given_to_the_builder_needs_no_key() {
    let store = Arc::new(MemoryCacheStore::new());
    assert!(stored(&store).cache_store(&MemoryEnvironment::new(&[])).is_ok());
}

/// A tombstone in `store` whose one cleanup's api no longer has a `cleanup`, so cleaning it
/// leaves a notice saying so.
fn tombstone(store: &Arc<MemoryCacheStore>) -> CacheEntry {
    let mut buried = entry(5, "users#create", "dev", 1);
    buried.cleanups.push(CachedCleanup { api: "users#gone".to_owned(), vars: CachedValues::new() });
    store.put(buried.clone()).unwrap();
    buried
}

/// The run's context under `engine`, with nothing planned, and what it was prepared from.
fn adhoc(engine: &Engine) -> (vc_executor::ActionCx, crate::plan::Prepared) {
    let prepared = super::unplanned(engine.layered(&[], &[]).unwrap());
    let inputs = RunInputs::new(ports::random(None).1, Arc::clone(&prepared.environment));
    (engine.action_cx(&prepared, (inputs, &Hosts::default())).unwrap(), prepared)
}

/// The sweep of a run of `engine` on `store` whose cache sent `events`.
fn sweep(engine: &Engine, store: &Arc<MemoryCacheStore>, events: Vec<CacheEvent>) -> Sweep {
    let (sender, received) = mpsc::unbounded_channel();
    for event in events {
        sender.send(event).unwrap();
    }
    let store: Arc<dyn CacheStore> = Arc::clone(store) as _;
    let cx = Arc::new(adhoc(engine).0);
    Sweep { cx, notices: vec!["started".to_owned()], buried: Some(Buried { store, events: received }) }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_end_of_a_run_cleans_up_each_tombstone_then_removes_it() {
    let store = Arc::new(MemoryCacheStore::new());
    let engine = stored(&store);
    let swept = sweep(&engine, &store, vec![CacheEvent::Replaced(tombstone(&store))]);
    let (_interrupter, mut interrupts) = interrupts();
    let notices = swept.finish(&mut interrupts).await;
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert_eq!(notices[0], "started");
    assert!(notices[1].contains("users#create") && notices[1].contains("no longer has a `cleanup`"), "{notices:?}");
    assert_eq!(store.list().unwrap(), Vec::new(), "a cleaned tombstone is removed");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_third_interrupt_leaves_the_tombstones_for_the_next_run() {
    let store = Arc::new(MemoryCacheStore::new());
    let engine = stored(&store);
    let buried = tombstone(&store);
    let swept = sweep(&engine, &store, vec![CacheEvent::Replaced(buried.clone())]);
    let (interrupter, mut interrupts) = interrupts();
    (0..3).for_each(|_| interrupter.interrupt());
    let notices = swept.finish(&mut interrupts).await;
    assert_eq!(notices.len(), 2, "{notices:?}");
    assert!(notices[1].contains("interrupted") && notices[1].contains("next run"), "{notices:?}");
    assert_eq!(store.list().unwrap(), vec![buried.listing], "the tombstone stays, for the next run to clean");
}

/// A notice the run's cache sent: a result it could not store.
const UNSTORED: &str = "cache: the cache failed: disk I/O error; `users#create` was not cached";

#[tokio::test(flavor = "multi_thread")]
async fn the_cache_notices_of_the_run_follow_the_start_ones_with_or_without_a_sweep() {
    let store = Arc::new(MemoryCacheStore::new());
    let engine = stored(&store);
    let (_interrupter, mut calm) = interrupts();
    let alone = sweep(&engine, &store, vec![CacheEvent::Notice(UNSTORED.to_owned())]).finish(&mut calm).await;
    assert_eq!(alone, ["started", UNSTORED], "a notice needs no tombstone");
    let (interrupter, mut interrupts) = interrupts();
    (0..3).for_each(|_| interrupter.interrupt());
    let events = vec![CacheEvent::Notice(UNSTORED.to_owned()), CacheEvent::Replaced(tombstone(&store))];
    let notices = sweep(&engine, &store, events).finish(&mut interrupts).await;
    assert_eq!(notices.len(), 3, "{notices:?}");
    assert_eq!(notices[1], UNSTORED, "an abandoned sweep keeps the run's notices");
    assert!(notices[2].contains("interrupted"), "{notices:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_run_start_counts_the_expired_entries_that_no_longer_open() {
    let engine = caching(unopened(&Arc::new(MemoryCacheStore::new()), 1));
    let (cx, prepared) = adhoc(&engine);
    let started = engine.cached(cx, &prepared).await;
    let notice = "cache: 1 entry sealed under another key was removed; its cleanups did not run";
    assert_eq!(started.notices, [notice]);
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_counts_the_entries_that_no_longer_open_and_removes_them() {
    let store = Arc::new(MemoryCacheStore::new());
    store.put(entry(1, "users#list", "", 1)).unwrap();
    let engine = caching(unopened(&store, 2));
    let (count, notices) = engine.cache_clear(None).await.unwrap();
    let notice = "cache: 2 entries sealed under another key were removed; their cleanups did not run";
    assert_eq!((count, notices), (3, vec![notice.to_owned()]));
    assert_eq!(store.list().unwrap(), Vec::new());
}
