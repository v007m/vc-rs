use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::json;
use vc_interfaces::{AuthError, Credentials};

use super::TokenCache;

fn counting(
    calls: &Arc<AtomicUsize>,
    lifetime: Option<Duration>,
) -> impl Fn() -> std::future::Ready<Result<Credentials, AuthError>> {
    let calls = Arc::clone(calls);
    move || {
        let turn = calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Ok(Credentials::header("authorization", format!("Bearer t{turn}")).expiring(lifetime)))
    }
}

fn token(credentials: &Credentials) -> &str {
    credentials.headers[0].1.expose()
}

#[test]
fn keys_depend_on_every_part_and_on_the_run_key_and_never_show_bytes() {
    let cache = TokenCache::new([1; 32]);
    let key = cache.key("svc", &json!({"user": "ada"}), &json!({"clientId": "shop"}));
    assert_eq!(key, cache.key("svc", &json!({"user": "ada"}), &json!({"clientId": "shop"})));
    assert_ne!(key, cache.key("svc", &json!({"user": "bob"}), &json!({"clientId": "shop"})));
    assert_ne!(key, cache.key("other", &json!({"user": "ada"}), &json!({"clientId": "shop"})));
    assert_ne!(key, cache.key("svc", &json!({"user": "ada"}), &json!({"clientId": "web"})));
    assert_ne!(key, TokenCache::new([2; 32]).key("svc", &json!({"user": "ada"}), &json!({"clientId": "shop"})));
    assert_ne!(key, TokenCache::default().key("svc", &json!({"user": "ada"}), &json!({"clientId": "shop"})));
    assert_eq!(format!("{key:?}"), "AuthKey(..)");
    assert!(!format!("{cache:?}").contains("svc"));
}

#[tokio::test]
async fn concurrent_requests_share_one_fetch() {
    let cache = Arc::new(TokenCache::new([0; 32]));
    let calls = Arc::new(AtomicUsize::new(0));
    let key = cache.key("svc", &json!({}), &json!({}));
    let fetch = counting(&calls, None);
    let (first, second) = tokio::join!(cache.get(key, Duration::ZERO, &fetch), cache.get(key, Duration::ZERO, &fetch));
    assert_eq!((token(&first.unwrap()), token(&second.unwrap())), ("Bearer t0", "Bearer t0"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn credentials_are_fetched_again_once_within_refresh_before_of_expiry() {
    let cache = TokenCache::new([0; 32]);
    let calls = Arc::new(AtomicUsize::new(0));
    let key = cache.key("svc", &json!({}), &json!({}));
    let fetch = counting(&calls, Some(Duration::from_secs(150)));
    let refresh = Duration::from_secs(60);
    assert_eq!(token(&cache.get(key, refresh, &fetch).await.unwrap()), "Bearer t0");
    tokio::time::advance(Duration::from_secs(89)).await;
    assert_eq!(token(&cache.get(key, refresh, &fetch).await.unwrap()), "Bearer t0");
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(token(&cache.get(key, refresh, &fetch).await.unwrap()), "Bearer t1");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn a_token_living_no_longer_than_refresh_before_is_refreshed_at_half_its_life() {
    let cache = TokenCache::new([0; 32]);
    let calls = Arc::new(AtomicUsize::new(0));
    let key = cache.key("svc", &json!({}), &json!({}));
    let fetch = counting(&calls, Some(Duration::from_secs(40)));
    let refresh = Duration::from_secs(60);
    assert_eq!(token(&cache.get(key, refresh, &fetch).await.unwrap()), "Bearer t0");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "one fetch for the first use, not two");
    tokio::time::advance(Duration::from_secs(19)).await;
    assert_eq!(token(&cache.get(key, refresh, &fetch).await.unwrap()), "Bearer t0");
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(token(&cache.get(key, refresh, &fetch).await.unwrap()), "Bearer t1");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn the_request_that_fetched_a_credential_uses_it_however_short_its_life() {
    let cache = TokenCache::new([0; 32]);
    let calls = Arc::new(AtomicUsize::new(0));
    let key = cache.key("svc", &json!({}), &json!({}));
    let fetch = counting(&calls, Some(Duration::ZERO));
    assert_eq!(token(&cache.get(key, Duration::from_secs(60), &fetch).await.unwrap()), "Bearer t0");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(token(&cache.get(key, Duration::from_secs(60), &fetch).await.unwrap()), "Bearer t1", "stale for the next");
}

#[tokio::test]
async fn a_failure_is_kept_for_the_run_until_invalidated() {
    let cache = TokenCache::new([0; 32]);
    let calls = Arc::new(AtomicUsize::new(0));
    let key = cache.key("svc", &json!({}), &json!({}));
    let fail = || {
        calls.fetch_add(1, Ordering::SeqCst);
        std::future::ready(Err(AuthError::Rejected { auth: "svc".into(), status: 401 }))
    };
    assert!(cache.get(key, Duration::ZERO, &fail).await.is_err());
    assert!(cache.get(key, Duration::ZERO, &fail).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    cache.invalidate(key);
    let fetch = counting(&calls, None);
    assert_eq!(token(&cache.get(key, Duration::ZERO, &fetch).await.unwrap()), "Bearer t1");
}
