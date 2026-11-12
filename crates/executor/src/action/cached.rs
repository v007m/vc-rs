use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CacheStoreError, CachedCleanup, Clock};
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, CacheSpec, Exchange, Program};

use super::{ActionExecutor, ActionRequest};
use crate::{ActionReport, CleanupCollector, PendingCleanup, Render, Saved};

/// A run's cross-run cache.
///
/// It holds the store, the clock its entries are made and expire by, and where what the run's
/// end must deal with goes: the tombstones of the entries fresh results replace, and the
/// store's problems, as notices.
#[derive(Clone)]
pub struct RunCache {
    store: Arc<dyn CacheStore>,
    clock: Arc<dyn Clock>,
    events: UnboundedSender<CacheEvent>,
}

impl std::fmt::Debug for RunCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunCache").finish_non_exhaustive()
    }
}

impl RunCache {
    /// The cache of a run keeping results in `store` by `clock`, sending its [`CacheEvent`]s to
    /// `events`, whose owner runs the tombstones' cleanups once every test is done, as their
    /// expiry would have, and reports the notices as warnings.
    #[must_use]
    pub fn new(store: Arc<dyn CacheStore>, clock: Arc<dyn Clock>, events: UnboundedSender<CacheEvent>) -> Self {
        Self { store, clock, events }
    }

    /// Sends `event` to the run's owner; one gone (the run is over) has no use for it.
    fn send(&self, event: CacheEvent) {
        drop(self.events.send(event));
    }
}

/// What a run's cache hands the run's owner while the run goes on.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CacheEvent {
    /// The tombstone of an entry a fresh result replaced, for the run's end.
    Replaced(CacheEntry),
    /// A store that failed, or a result that could not be stored: a warning, never a failure.
    /// It names the api or test and the store's error, never an entry's values.
    Notice(String),
}

/// Runs `call:` and `test:` actions with `cache:` through the cross-run cache.
///
/// A decorator around the kind's executor. A live entry under the action's key stands in for the action
/// once its `cache.check` passes, whose exchanges the action reports before its own, hit or
/// miss; otherwise the action runs and, when it passes, its saves and
/// cleanups are stored for `cache.for`. A stored action's cleanups run when its entry expires,
/// is cleared or is replaced by a fresh one, not when its frame closes; a replaced entry's wait
/// for the end of the run as a tombstone in the store, since another instance may still use
/// what it made.
/// Without a store, or for an action without `cache:`, it only runs the wrapped executor; a
/// store that fails never fails the action, and the run's end reports it as a notice.
#[derive(Clone)]
pub struct CachedExecutor {
    inner: Arc<dyn ActionExecutor>,
}

impl std::fmt::Debug for CachedExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedExecutor").finish_non_exhaustive()
    }
}

/// A cached action: the run's cache, its `cache:`, the entry it would store (key and listing,
/// no values yet) and its `id`, which its saves go under.
struct Cacheable {
    cache: RunCache,
    spec: CacheSpec,
    entry: CacheEntry,
    id: Option<String>,
}

impl CachedExecutor {
    /// `inner`, through the cache.
    #[must_use]
    pub fn new(inner: Arc<dyn ActionExecutor>) -> Self {
        Self { inner }
    }

    /// The cached saves, if a live entry holds them and its check passes, and the check's
    /// exchanges, which the action reports either way. The check reads the saves as saved
    /// values, and under the action's `id` (`${var.admin.userId}`).
    async fn hit(request: &ActionRequest, cacheable: &Cacheable) -> (Option<Saved>, Vec<Exchange>) {
        let stored = cacheable.cache.store.get(&cacheable.entry.key, cacheable.entry.listing.created_ms);
        let Some(entry) = stored.unwrap_or_else(|error| cacheable.unread(&error)) else { return (None, Vec::new()) };
        let Some(check) = cacheable.spec.check else { return (Some(entry.saved), Vec::new()) };
        let mut frame = request.frame.clone();
        let saved = Arc::make_mut(&mut frame.saved);
        saved.extend(entry.saved.clone());
        if let Some(id) = &cacheable.id {
            saved.insert(id.clone(), Value::Object(entry.saved.clone()));
        }
        let checked = request.cx.executors.execute(ActionRequest { action: check, frame, ..request.clone() }).await;
        (matches!(checked.outcome, ActionOutcome::Passed).then_some(entry.saved), checked.exchanges)
    }

    /// Runs the action and, when it passes, stores its saves and cleanups; stored cleanups
    /// leave the report, so they wait for the entry to expire instead of the frame to close.
    /// Its parts hand their cleanups to a collector of its own, so every one can be stored. The
    /// tombstone of the entry it replaces (its check failed, or it expired since the run began)
    /// goes to the run's owner, for the run's end. A result with a cleanup that is no api's, or
    /// one the store cannot write, is not stored, keeps its cleanups and sends a notice.
    async fn miss(&self, request: ActionRequest, cacheable: Cacheable) -> ActionReport {
        let program = Arc::clone(&request.cx.program);
        let mut report = CleanupCollector::around(request, |request| self.inner.execute(request)).await;
        if !matches!(report.outcome, ActionOutcome::Passed) {
            return report;
        }
        let cleanups = report.cleanups.iter().map(|cleanup| stored_cleanup(&program, cleanup));
        let stored = match cleanups.collect::<Result<Vec<_>, _>>() {
            Ok(cleanups) => {
                cacheable.store(CacheEntry { saved: report.saved.clone(), cleanups, ..cacheable.entry.clone() })
            }
            Err(cleanup) => cacheable.unstorable(&cleanup),
        };
        if stored {
            report.cleanups.clear();
        }
        report
    }
}

impl Cacheable {
    /// A notice for the run's end: `cache: <text>`.
    fn notice(&self, text: &str) {
        self.cache.send(CacheEvent::Notice(format!("cache: {text}")));
    }

    /// No entry, after the store failed to read one: the action runs again.
    fn unread(&self, error: &CacheStoreError) -> Option<CacheEntry> {
        self.notice(&format!("could not read the entry: {error}; `{}` ran again", self.entry.listing.label));
        None
    }

    /// Not stored, for the cleanup `cleanup` names, which is no api's.
    fn unstorable(&self, cleanup: &str) -> bool {
        let label = &self.entry.listing.label;
        let why = "is no api's `cleanup`, so it cannot be stored";
        self.notice(&format!("the cleanup `{cleanup}` {why}; `{label}` was not cached"));
        false
    }

    /// Puts `entry` and hands over the tombstone of the entry it replaced; whether it is stored.
    fn store(&self, entry: CacheEntry) -> bool {
        match self.cache.store.put(entry) {
            Ok(replaced) => {
                if let Some(buried) = replaced {
                    self.cache.send(CacheEvent::Replaced(buried));
                }
                true
            }
            Err(error) => {
                self.notice(&format!("{error}; `{}` was not cached", self.entry.listing.label));
                false
            }
        }
    }
}

#[async_trait]
impl ActionExecutor for CachedExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        let Some(cacheable) = cacheable(&request) else { return self.inner.execute(request).await };
        let (hit, checked) = Self::hit(&request, &cacheable).await;
        if let Some(saved) = hit {
            return ActionReport { saved, exchanges: checked, ..ActionReport::passed() };
        }
        let mut report = self.miss(request, cacheable).await;
        report.exchanges.splice(0..0, checked);
        report
    }

    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId> {
        let check = action.cache.as_ref().and_then(|spec| spec.check);
        self.inner.follows(program, action).into_iter().chain(check).collect()
    }
}

/// The action as cached, when it has `cache:` and the run has a store. Its key holds the vars
/// the action writes, rendered; the clock's now is both when the entry is made and the time
/// a stored one must still be live at.
fn cacheable(request: &ActionRequest) -> Option<Cacheable> {
    let cache = request.cx.cache.clone()?;
    let action = request.cx.action(request.action)?;
    let spec = action.cache?;
    let program = &request.cx.program;
    let label = label(program, action)?;
    let own = request.cx.values.action(request.action).unwrap_or_default();
    let vars = Render::new(request).ok()?.vars(own.keys());
    let profiles = request.cx.values.profile_names().join(",");
    let created_ms = cache.clock.now_ms();
    let expires_ms = created_ms.saturating_add(u64::try_from(spec.ttl.get().as_millis()).unwrap_or(u64::MAX));
    let key = key(&label, &vars, &profiles);
    let listing = CacheListing { label, profiles, created_ms, expires_ms };
    let entry = CacheEntry { key, listing, saved: Saved::new(), cleanups: Vec::new() };
    let id = action.id.and_then(|id| program.str(id)).map(str::to_owned);
    Some(Cacheable { cache, spec, entry, id })
}

/// The api or test the action runs: `users#create`.
fn label(program: &Program, action: &Action) -> Option<String> {
    let sym = match &action.kind {
        ActionKind::Call(api) => api.sym(),
        ActionKind::Test(test) => test.sym(),
        ActionKind::Function(_) | ActionKind::Command(_) | ActionKind::Inline(_) => return None,
    };
    program.str(sym).map(str::to_owned)
}

/// blake3 of the label, the profiles and the vars as JSON (names in order), each
/// length-prefixed.
fn key(label: &str, vars: &Saved, profiles: &str) -> CacheKey {
    let vars = Value::Object(vars.clone()).to_string();
    let mut hasher = blake3::Hasher::new_derive_key("vibranium cache.db key v1");
    for part in [label, profiles, vars.as_str()] {
        hasher.update(&u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes()).update(part.as_bytes());
    }
    CacheKey::new(*hasher.finalize().as_bytes())
}

/// `cleanup` as stored: by the api whose `cleanup` it is, since action IDs change between
/// compiles. One that is no api's cleanup is an error naming what it runs, and then the action
/// is not stored.
fn stored_cleanup(program: &Program, cleanup: &PendingCleanup) -> Result<CachedCleanup, String> {
    let api = program.apis.values().find(|api| api.cleanup == Some(cleanup.action));
    let Some(name) = api.and_then(|api| program.str(api.id.sym())) else { return Err(cleanup_name(program, cleanup)) };
    Ok(CachedCleanup { api: name.to_owned(), vars: Saved::clone(&cleanup.vars) })
}

/// What `cleanup` runs, for a notice: the api or test it calls, else its action's place.
fn cleanup_name(program: &Program, cleanup: &PendingCleanup) -> String {
    let action = usize::try_from(cleanup.action.index()).ok().and_then(|index| program.actions.get(index));
    action.and_then(|action| label(program, action)).unwrap_or_else(|| format!("action {}", cleanup.action.index()))
}

#[cfg(test)]
#[path = "cached_tests.rs"]
mod tests;
