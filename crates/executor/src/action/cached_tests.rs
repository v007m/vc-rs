use std::sync::Arc;
use std::time::Duration as StdDuration;

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use vc_interfaces::{CacheEntry, CacheStore, CachedCleanup, Clock};
use vc_model::{ActionId, CacheSpec, Date, Duration, HttpResponse, OwnerKey, Program};
use vc_testkit::{FakeTransport, MemoryCacheStore, Recorded, RecordingTransport};

use super::{CachedExecutor, RunCache, key};
use crate::fixture::{self, Fixture};
use crate::{
    ActionCx, ActionExecutor, ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PipelineParts, Saved,
    TestExecutor,
};

fn json_response(code: u16, body: &Value) -> HttpResponse {
    let body = Bytes::from(body.to_string());
    let headers = HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_static("application/json"))]);
    HttpResponse { status: StatusCode::from_u16(code).unwrap(), headers, wire_size: body.len() as u64, body }
}

fn saved(value: Value) -> Saved {
    let Value::Object(map) = value else { unreachable!("an object") };
    map
}

fn created(id: u64) -> Vec<HttpResponse> {
    vec![json_response(201, &json!({ "id": id }))]
}

/// A `POST /users` (api `users#n1`) saving `userId`, whose api's cleanup (`users#n0`, to
/// `/users/${var.userId}`) deletes it. The action has the id `created`, the var `region` and `cache: { for: 1s }`, with a
/// `check` calling `check_path` when given.
fn creating(region: &str, check_path: Option<&str>) -> (Arc<Program>, ActionId) {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users");
    fixture.action_mut(delete).vars.values = fixture.vars(&[("userId", "${response.body.id}")]);
    fixture.api_mut(delete).path = fixture.template("/users/${var.userId}");
    let create = fixture.creating("/users", delete);
    fixture.action_mut(create).id = Some(fixture.sym("created"));
    fixture.action_mut(create).vars.values = fixture.vars(&[("region", region)]);
    fixture.action_mut(create).save.values = fixture.vars(&[("userId", "${response.body.id}")]);
    let check = check_path.map(|path| {
        let path = fixture.template(path);
        let get = fixture.call("/users");
        fixture.api_mut(get).path = path;
        get
    });
    fixture.action_mut(create).cache = Some(CacheSpec { ttl: Duration::new(StdDuration::from_secs(1)), check });
    (Arc::new(fixture.program), create)
}

/// A clock stopped at the given ms past the epoch.
struct StoppedClock(u64);

impl Clock for StoppedClock {
    fn today(&self) -> Date {
        Date::new(2026, 11, 9).unwrap()
    }

    fn now_ms(&self) -> u64 {
        self.0
    }
}

/// Runs `action` with `store` as its cache and the clock at `at`, against a transport
/// answering `answers` in turn. The run itself started at 0, so expiry goes by the clock.
async fn run(
    target: &(Arc<Program>, ActionId),
    cache: (Option<&MemoryCacheStore>, u64),
    answers: Vec<HttpResponse>,
) -> (ActionReport, Vec<Recorded>) {
    let (report, sent, _) = replacing(target, cache, answers).await;
    (report, sent)
}

/// [`run`], also giving the tombstones of the entries fresh results replaced, left for the run's end.
async fn replacing(
    (program, action): &(Arc<Program>, ActionId),
    (store, at): (Option<&MemoryCacheStore>, u64),
    answers: Vec<HttpResponse>,
) -> (ActionReport, Vec<Recorded>, Vec<CacheEntry>) {
    let fake = FakeTransport::scripted(answers.into_iter().map(Ok).collect());
    let (transport, recordings) = RecordingTransport::new(Arc::new(fake));
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(transport), HostLimits::spawn(8)));
    let inputs = fixture::inputs(&[]).with_started_at(0);
    let cx = ActionCx::new(Arc::clone(program), Arc::new(executors), inputs);
    let (replaced, mut waiting) = mpsc::unbounded_channel();
    let cx = match store {
        Some(store) => cx.with_cache(RunCache::new(Arc::new(store.clone()), Arc::new(StoppedClock(at)), replaced)),
        None => cx,
    };
    let cx = Arc::new(cx);
    let request = ActionRequest { action: *action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    let report = cx.executors.execute(request).await;
    (report, recordings.take(), std::iter::from_fn(|| waiting.try_recv().ok()).collect())
}

/// The path of each exchange `report` gives, in order.
fn reported(report: &ActionReport) -> Vec<String> {
    report.exchanges.iter().map(|exchange| exchange.request.uri.path().to_owned()).collect()
}

#[tokio::test]
async fn a_passed_action_is_stored_with_its_cleanups_and_reused_until_it_expires() {
    let store = MemoryCacheStore::new();
    let target = creating("eu", None);
    let (first, sent) = run(&target, (Some(&store), 1_000), created(7)).await;
    assert_eq!((first.outcome.failure(), first.saved, sent.len()), (None, saved(json!({ "userId": 7 })), 1));
    assert_eq!(first.cleanups, vec![], "the cleanup waits for the entry to expire");
    let (second, sent) = run(&target, (Some(&store), 1_999), created(8)).await;
    assert_eq!((second.outcome.failure(), second.saved, sent.len()), (None, saved(json!({ "userId": 7 })), 0));
    let expired = store.take_expired(2_000).unwrap();
    let cleanup = CachedCleanup { api: "users#n1".into(), vars: saved(json!({ "userId": 7 })) };
    let listing = expired.first().map(|entry| entry.listing.clone()).unwrap_or_default();
    assert_eq!((listing.label.as_str(), listing.profiles.as_str()), ("users#n1", ""));
    assert_eq!((listing.created_ms, listing.expires_ms), (1_000, 2_000));
    assert_eq!(expired.into_iter().map(|entry| entry.cleanups).collect::<Vec<_>>(), vec![vec![cleanup]]);
    let (third, sent) = run(&target, (Some(&store), 2_000), created(8)).await;
    assert_eq!((third.saved, sent.len()), (saved(json!({ "userId": 8 })), 1), "an expired entry is not used");
}

#[tokio::test]
async fn other_vars_miss_and_a_failed_action_is_not_stored() {
    let store = MemoryCacheStore::new();
    let failed = run(&creating("eu", None), (Some(&store), 1_000), vec![json_response(500, &json!({}))]).await;
    assert!(failed.0.outcome.failure().is_some());
    assert_eq!(store.list().map(|listings| listings.len()), Ok(0), "a failed action is not stored");
    let (_, sent) = run(&creating("eu", None), (Some(&store), 1_000), created(7)).await;
    assert_eq!(sent.len(), 1);
    let (other, sent) = run(&creating("us", None), (Some(&store), 1_000), created(9)).await;
    assert_eq!((other.saved, sent.len()), (saved(json!({ "userId": 9 })), 1));
    assert_eq!(store.list().map(|listings| listings.len()), Ok(2));
}

#[tokio::test]
async fn the_check_reads_the_cached_saves_and_a_failed_check_reruns_the_action_and_hands_over_the_old_entry() {
    let store = MemoryCacheStore::new();
    let target = creating("eu", Some("/users/${var.userId}/${var.created.userId}"));
    let (_, sent) = run(&target, (Some(&store), 1_000), created(7)).await;
    assert_eq!(sent.len(), 1, "no check before anything is cached");
    let (hit, sent) = run(&target, (Some(&store), 1_001), vec![json_response(200, &json!({}))]).await;
    let paths: Vec<_> = sent.iter().map(|recorded| recorded.request.uri.path().to_owned()).collect();
    assert_eq!((hit.saved, paths), (saved(json!({ "userId": 7 })), vec!["/users/7/7".to_owned()]));
    assert_eq!(reported(&hit), ["/users/7/7"], "a hit reports its check's exchange");
    let answers = vec![json_response(404, &json!({})), json_response(201, &json!({ "id": 8 }))];
    let (rerun, sent, replaced) = replacing(&target, (Some(&store), 1_002), answers).await;
    assert_eq!(reported(&rerun), ["/users/7/7", "/users"], "a miss reports the check's exchange, then its own");
    let paths: Vec<_> = sent.iter().map(|recorded| recorded.request.uri.path().to_owned()).collect();
    assert_eq!(
        (rerun.saved, paths),
        (saved(json!({ "userId": 8 })), ["/users/7/7", "/users"].map(String::from).to_vec()),
        "the old entry's cleanup waits for the run's end, as another instance may still use what it made"
    );
    let cleanup = CachedCleanup { api: "users#n1".into(), vars: saved(json!({ "userId": 7 })) };
    assert_eq!(replaced.iter().map(|entry| entry.cleanups.clone()).collect::<Vec<_>>(), vec![vec![cleanup]]);
    let buried = store.take_expired(1_002).unwrap();
    assert_eq!(buried, replaced, "until its cleanups run, the old entry stays in the store, expired");
    let kept: Saved = store.take_expired(u64::MAX).unwrap().into_iter().flat_map(|entry| entry.saved).collect();
    assert_eq!(kept, saved(json!({ "userId": 8 })), "the fresh entry is the one kept");
}

#[tokio::test]
async fn without_a_store_the_action_runs_every_time_and_keeps_its_cleanups() {
    let target = creating("eu", None);
    for id in [7, 8] {
        let (report, sent) = run(&target, (None, 1_000), created(id)).await;
        assert_eq!((report.saved, report.cleanups.len(), sent.len()), (saved(json!({ "userId": id })), 1, 1));
    }
}

#[test]
fn the_check_is_reached_from_the_cached_action() {
    let (program, create) = creating("eu", Some("/users/${var.userId}"));
    let action = &program.actions[usize::try_from(create.index()).unwrap()];
    let check = action.cache.and_then(|spec| spec.check);
    assert_eq!(
        CachedExecutor::new(Arc::new(TestExecutor)).follows(&program, action),
        check.into_iter().collect::<Vec<_>>()
    );
}

#[test]
fn keys_differ_by_label_vars_and_profiles() {
    let vars = saved(json!({ "role": "admin" }));
    let base = key("users#create", &vars, "dev");
    assert_eq!(base, key("users#create", &vars, "dev"));
    let others = [
        key("users#update", &vars, "dev"),
        key("users#create", &saved(json!({ "role": "member" })), "dev"),
        key("users#create", &vars, "prod"),
        key("users#create", &Saved::new(), "dev"),
    ];
    assert!(others.iter().all(|other| *other != base), "{others:?}");
    assert_ne!(key("ab", &Saved::new(), ""), key("a", &Saved::new(), "b"), "the parts are length-prefixed");
}
