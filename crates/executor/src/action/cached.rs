use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use vc_interfaces::{CacheEntry, CacheKey, CacheListing, CacheStore, CachedCleanup};
use vc_model::{Action, ActionId, ActionKind, ActionOutcome, CacheSpec, Program};

use super::{ActionExecutor, ActionRequest};
use crate::{ActionReport, CleanupCollector, PendingCleanup, Render, Saved};

/// Runs `call:` and `test:` actions with `cache:` through the cross-run cache.
///
/// A decorator around the kind's executor. A live entry under the action's key stands in for the action
/// once its `cache.check` passes; otherwise the action runs and, when it passes, its saves and
/// cleanups are stored for `cache.for`. A stored action's cleanups run when its entry expires
/// or is cleared, not when its frame closes. Without a store, or for an action without
/// `cache:`, it only runs the wrapped executor; a store that fails never fails the action.
#[derive(Clone)]
pub struct CachedExecutor {
    inner: Arc<dyn ActionExecutor>,
}

impl std::fmt::Debug for CachedExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedExecutor").finish_non_exhaustive()
    }
}

/// A cached action: the store, its `cache:`, the entry it would store (key and listing, no
/// values yet) and its `id`, which its saves go under.
struct Cacheable {
    store: Arc<dyn CacheStore>,
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

    /// The cached saves, if a live entry holds them and its check passes. The check reads them
    /// as saved values, and under the action's `id` (`${var.admin.userId}`).
    async fn hit(request: &ActionRequest, cacheable: &Cacheable) -> Option<Saved> {
        let entry = cacheable.store.get(&cacheable.entry.key, now_ms(request)).ok()??;
        let Some(check) = cacheable.spec.check else { return Some(entry.saved) };
        let mut frame = request.frame.clone();
        let saved = Arc::make_mut(&mut frame.saved);
        saved.extend(entry.saved.clone());
        if let Some(id) = &cacheable.id {
            saved.insert(id.clone(), Value::Object(entry.saved.clone()));
        }
        let checked = request.cx.executors.execute(ActionRequest { action: check, frame, ..request.clone() }).await;
        matches!(checked.outcome, ActionOutcome::Passed).then_some(entry.saved)
    }

    /// Runs the action and, when it passes, stores its saves and cleanups; stored cleanups
    /// leave the report, so they wait for the entry to expire instead of the frame to close.
    /// Its parts hand their cleanups to a collector of its own, so every one can be stored.
    async fn miss(&self, request: ActionRequest, cacheable: Cacheable) -> ActionReport {
        let program = Arc::clone(&request.cx.program);
        let mut kept = CleanupCollector::under(request.frame.sink.clone());
        let mut report = self.inner.execute(ActionRequest { frame: kept.frame(&request.frame), ..request }).await;
        report.cleanups = kept.take(report.cleanups);
        if !matches!(report.outcome, ActionOutcome::Passed) {
            return report;
        }
        let cleanups = report.cleanups.iter().map(|cleanup| stored_cleanup(&program, cleanup));
        let Some(cleanups) = cleanups.collect::<Option<Vec<_>>>() else { return report };
        let entry = CacheEntry { saved: report.saved.clone(), cleanups, ..cacheable.entry };
        if cacheable.store.put(entry).is_ok() {
            report.cleanups.clear();
        }
        report
    }
}

#[async_trait]
impl ActionExecutor for CachedExecutor {
    async fn execute(&self, request: ActionRequest) -> ActionReport {
        let Some(cacheable) = cacheable(&request) else { return self.inner.execute(request).await };
        if let Some(saved) = Self::hit(&request, &cacheable).await {
            return ActionReport { saved, ..ActionReport::passed() };
        }
        self.miss(request, cacheable).await
    }

    fn follows(&self, program: &Program, action: &Action) -> Vec<ActionId> {
        let check = action.cache.as_ref().and_then(|spec| spec.check);
        self.inner.follows(program, action).into_iter().chain(check).collect()
    }
}

/// The action as cached, when it has `cache:` and the run has a store. Its key holds the vars
/// the action writes, rendered.
fn cacheable(request: &ActionRequest) -> Option<Cacheable> {
    let store = request.cx.cache.clone()?;
    let action = request.cx.action(request.action)?;
    let spec = action.cache?;
    let program = &request.cx.program;
    let label = label(program, action)?;
    let own = request.cx.values.action(request.action).unwrap_or_default();
    let vars = Render::new(request).ok()?.vars(own.keys());
    let profiles = request.cx.values.profile_names().join(",");
    let created_ms = now_ms(request);
    let expires_ms = created_ms.saturating_add(u64::try_from(spec.ttl.get().as_millis()).unwrap_or(u64::MAX));
    let key = key(&label, &vars, &profiles);
    let listing = CacheListing { label, profiles, created_ms, expires_ms };
    let entry = CacheEntry { key, listing, saved: Saved::new(), cleanups: Vec::new() };
    let id = action.id.and_then(|id| program.str(id)).map(str::to_owned);
    Some(Cacheable { store, spec, entry, id })
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

/// The run's start, which every cache read and write of the run uses as now.
fn now_ms(request: &ActionRequest) -> u64 {
    request.cx.values.inputs().started_ms
}

/// `cleanup` as stored: by the api whose `cleanup` it is, since action IDs change between
/// compiles; `None` for one that is no api's cleanup, and then the action is not stored.
fn stored_cleanup(program: &Program, cleanup: &PendingCleanup) -> Option<CachedCleanup> {
    let api = program.apis.values().find(|api| api.cleanup == Some(cleanup.action))?;
    let name = program.str(api.id.sym())?.to_owned();
    Some(CachedCleanup { api: name, vars: Saved::clone(&cleanup.vars) })
}

#[cfg(test)]
#[path = "cached_tests.rs"]
mod tests;
