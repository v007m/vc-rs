//! The cross-run cache (`.vibranium/cache.db`): opening it under its key, running the cleanups
//! of the entries that expired or were cleared, and `vc cache list` and `vc cache clear`.
//!
//! The key is `VIB_CACHE_KEY` when it is set, else the one the OS keychain holds, made and
//! stored there on first use. With neither, the cache is off for the run: `cache:` actions run
//! every time and the run says so in a V5113 notice. An entry sealed under another key, or
//! changed on disk, reads as a miss. A cache that cannot be opened or read never fails a run.

use std::fmt::Write as _;
use std::sync::Arc;

use serde_json::{Value, json};

use vc_adapter_store::{CacheSecret, SqliteCache};
use vc_diagnostics::Code;
use vc_executor::{ActionCx, ActionRequest, RunInputs};
use vc_interfaces::{CacheEntry, CacheListing, CacheStore, CacheStoreError, CachedCleanup, Environment, Secret};
use vc_model::{ActionId, Plan, Program};

use crate::plan::Prepared;
use crate::ports::{self, Layered};
use crate::stored::started;
use crate::{Engine, EngineError};

/// The cache entries, oldest first, as `vc cache list` shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheReport {
    entries: Vec<CacheListing>,
    now_ms: u64,
}

/// Where the cache lives, under the project root.
const CACHE: &str = ".vibranium/cache.db";
/// The variable that gives the key: 64 hex digits.
const KEY_VAR: &str = "VIB_CACHE_KEY";
/// The keychain entry that holds the key.
const SERVICE: &str = "vibranium";
const ACCOUNT: &str = "cache-key";

impl Engine {
    /// `cx` keeping `cache:`d results in the cross-run cache, once the cleanups of the entries
    /// expired at the run's start have run, and the notices of a cache that is off or failed.
    /// A program with no `cache:` action leaves the cache, and the keychain, alone.
    pub(crate) async fn cached(&self, cx: ActionCx, prepared: &Prepared) -> (Arc<ActionCx>, Vec<String>) {
        if !prepared.program.actions.iter().any(|action| action.cache.is_some()) {
            return (Arc::new(cx), Vec::new());
        }
        let store = match self.cache_store(prepared.environment.as_ref()) {
            Ok(store) => store,
            Err(error) => return (Arc::new(cx), vec![off(&error)]),
        };
        let cx = Arc::new(cx.with_cache(Arc::clone(&store)));
        let notices = match store.take_expired(cx.values.inputs().started_ms) {
            Ok(expired) => Cleaner::new(&cx).clean(expired).await,
            Err(error) => vec![off(&store_error(&error))],
        };
        (cx, notices)
    }

    /// Every cache entry, oldest first: what `vc cache list` shows.
    ///
    /// # Errors
    ///
    /// [`EngineError::Cache`] when there is no key or the cache cannot be read, and the errors
    /// of compiling the project.
    pub fn cache_list(&self) -> Result<CacheReport, EngineError> {
        let layered = self.layered(&[], &[])?;
        let store = self.cache_store(layered.environment.as_ref())?;
        let entries = store.list().map_err(|error| store_error(&error))?;
        Ok(CacheReport { entries, now_ms: self.clock.now_ms() })
    }

    /// Removes the entries of `label` (`users#create`), or every entry, running their cleanups
    /// under the profiles each was made with; gives back how many went and a notice per
    /// cleanup that failed. Call it from within a Tokio runtime.
    ///
    /// # Errors
    ///
    /// [`EngineError::Cache`] when there is no key or the cache cannot be written,
    /// [`EngineError::Setting`] when the declared secrets cannot be loaded, and the errors of
    /// compiling the project.
    pub async fn cache_clear(&self, label: Option<&str>) -> Result<(usize, Vec<String>), EngineError> {
        let layered = self.layered(&[], &[])?;
        let store = self.cache_store(layered.environment.as_ref())?;
        let prepared = unplanned(layered);
        let secrets = self.secrets(&prepared.program.config.secrets, &prepared.environment).await?;
        let inputs = RunInputs::new(ports::random(None).1, Arc::clone(&prepared.environment))
            .with_secrets(secrets)
            .with_started_at(self.clock.now_ms());
        let cx = Arc::new(ActionCx::new(Arc::clone(&prepared.program), self.executors(&prepared)?, inputs));
        let cleared = store.clear(label).map_err(|error| store_error(&error))?;
        let count = cleared.len();
        Ok((count, Cleaner::new(&cx).clean(cleared).await))
    }

    /// The builder's store, else `.vibranium/cache.db` under the key.
    fn cache_store(&self, environment: &dyn Environment) -> Result<Arc<dyn CacheStore>, EngineError> {
        if let Some(store) = &self.cache {
            return Ok(Arc::clone(store));
        }
        let secret = self.cache_secret(environment)?;
        let store = SqliteCache::open(&self.root().join(CACHE), &secret).map_err(|error| store_error(&error))?;
        Ok(Arc::new(store))
    }

    /// `VIB_CACHE_KEY`, else the keychain's key, made and stored there when it holds none.
    fn cache_secret(&self, environment: &dyn Environment) -> Result<CacheSecret, EngineError> {
        if let Some(text) = environment.var(KEY_VAR) {
            return CacheSecret::parse(&text).ok_or_else(|| no_key(&format!("`{KEY_VAR}` is not 64 hex digits")));
        }
        let keychain = self.keychain.as_ref().ok_or_else(|| no_key(&format!("no `{KEY_VAR}` and no OS keychain")))?;
        let unusable = |reason: String| no_key(&format!("the OS keychain cannot be used: {reason}"));
        let stored = keychain.get(SERVICE, ACCOUNT).map_err(|error| unusable(error.reason))?;
        if let Some(stored) = stored {
            return CacheSecret::parse(stored.expose()).ok_or_else(|| unusable("its key is damaged".to_owned()));
        }
        let fresh = CacheSecret::generate().ok_or_else(|| unusable("no random bytes for a key".to_owned()))?;
        keychain.set(SERVICE, ACCOUNT, &Secret::new(fresh.to_hex().as_str())).map_err(|e| unusable(e.reason))?;
        Ok(fresh)
    }
}

impl CacheReport {
    /// One line per entry: what it caches, its profiles and when it expires.
    #[must_use]
    pub fn text(&self) -> String {
        if self.entries.is_empty() {
            return "the cache is empty\n".to_owned();
        }
        let mut out = String::new();
        for entry in &self.entries {
            let state = if entry.expires_ms <= self.now_ms { "expired" } else { "expires" };
            let _ = write!(out, "{}  {state} {}", entry.label, started(entry.expires_ms));
            if !entry.profiles.is_empty() {
                let _ = write!(out, "  profile {}", entry.profiles);
            }
            out.push('\n');
        }
        out
    }

    /// The entries as JSON, for scripts and agents; values are never listed.
    #[must_use]
    pub fn json(&self) -> Value {
        let entry = |entry: &CacheListing| {
            let profiles: Vec<&str> = entry.profiles.split(',').filter(|name| !name.is_empty()).collect();
            json!({ "label": entry.label, "profiles": profiles, "createdMs": entry.created_ms, "expiresMs": entry.expires_ms })
        };
        Value::Array(self.entries.iter().map(entry).collect())
    }
}

/// Runs the stored cleanups of cache entries, each entry under the profiles it was made with.
struct Cleaner<'c> {
    cx: &'c Arc<ActionCx>,
}

impl<'c> Cleaner<'c> {
    const fn new(cx: &'c Arc<ActionCx>) -> Self {
        Self { cx }
    }

    /// Runs the cleanups of `entries`, oldest entry first and each entry's newest cleanup first;
    /// a notice for each one that failed or whose api no longer has a cleanup.
    async fn clean(&self, entries: Vec<CacheEntry>) -> Vec<String> {
        let mut notices = Vec::new();
        for entry in entries {
            let cx = self.under(&entry.listing.profiles);
            for cleanup in entry.cleanups.iter().rev() {
                if let Err(problem) = run_cleanup(&cx, cleanup).await {
                    notices.push(format!("cache: the cleanup of {} failed: {problem}", entry.listing.label));
                }
            }
        }
        notices
    }

    /// A context like the run's under `profiles` (comma-separated names), so a cleanup goes to
    /// the host its entry was made on.
    fn under(&self, profiles: &str) -> Arc<ActionCx> {
        let program = &self.cx.program;
        let named =
            |name: &str| program.profiles.values().find(|p| program.str(p.id.sym()) == Some(name)).map(|p| p.id);
        let ids = profiles.split(',').filter(|name| !name.is_empty()).filter_map(named).collect();
        let inputs = self.cx.values.inputs().clone().with_profiles(ids);
        Arc::new(ActionCx::new(Arc::clone(program), Arc::clone(&self.cx.executors), inputs))
    }
}

/// Runs one stored cleanup: its api's `cleanup` with the stored vars.
async fn run_cleanup(cx: &Arc<ActionCx>, cleanup: &CachedCleanup) -> Result<(), String> {
    let action = cleanup_of(&cx.program, &cleanup.api)
        .ok_or_else(|| format!("`{}` no longer has a `cleanup`, so what it made is left in place", cleanup.api))?;
    let request = ActionRequest::cleanup(cx, action, &Arc::new(cleanup.vars.clone()));
    let report = cx.executors.execute(request).await;
    report.outcome.failure().map_or(Ok(()), |failure| Err(failure.message.clone()))
}

/// The `cleanup` action of the api whose full ID is `api`.
fn cleanup_of(program: &Program, api: &str) -> Option<ActionId> {
    program.apis.values().find(|found| program.str(found.id.sym()) == Some(api)).and_then(|found| found.cleanup)
}

/// The settings and environment of `layered` with nothing planned, for actions run outside a
/// run.
fn unplanned(layered: Layered) -> Prepared {
    let Layered { program, notices, settings, environment, dot_env } = layered;
    let (plan, profiles, names) = (Arc::new(Plan::default()), Vec::new(), Vec::new());
    Prepared { program, plan, settings, profiles, names, notices, environment, dot_env }
}

/// The cache has no key: V5113.
fn no_key(reason: &str) -> EngineError {
    EngineError::Cache {
        reason: format!("{}: no cache key: {reason}", Code::V5113),
        help: Some(format!("set `{KEY_VAR}` to 64 hex digits, as CI does, or run where the OS keychain is unlocked")),
    }
}

fn store_error(error: &CacheStoreError) -> EngineError {
    EngineError::Cache { reason: error.to_string(), help: None }
}

/// A cache problem as a run's notice: the run goes on with `cache:` actions run every time.
fn off(error: &EngineError) -> String {
    let help = error.help().map(|help| format!(" ({help})")).unwrap_or_default();
    format!("{error}; `cache:` actions run every time this run{help}")
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
