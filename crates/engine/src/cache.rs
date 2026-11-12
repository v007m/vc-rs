//! The cross-run cache (`.vibranium/cache.db`): opening it under its key, running the cleanups
//! of the entries that expired, were cleared or were replaced (their tombstones), and
//! `vc cache list` and `vc cache clear`.
//!
//! The key is `VIB_CACHE_KEY` when it is set, else the one the OS keychain holds, made and
//! stored there on first use. With neither, the cache is off for the run: `cache:` actions run
//! every time and the run says so in a V5113 notice. An entry sealed under another key, or
//! changed on disk, reads as a miss. A cache that cannot be opened or read never fails a run.

use std::fmt::Write as _;
use std::sync::Arc;

use serde_json::{Value, json};

use tokio::sync::mpsc::{self, UnboundedReceiver};
use vc_adapter_store::{CacheSecret, SqliteCache};
use vc_diagnostics::Code;
use vc_executor::{ActionCx, ActionRequest, RunCache};
use vc_interfaces::{CacheEntry, CacheListing, CacheStore, CacheStoreError, CachedCleanup, Environment, Secret};
use vc_model::Plan;

use crate::examples::Draw;
use crate::plan::Prepared;
use crate::ports::{Layered, SecretSource};
use crate::stored::started;
use crate::{Engine, EngineError, Interrupts};

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

/// A run's context and what its cache leaves for the run's end: the notices of its start, and
/// the tombstones of the entries fresh results replace during the run, whose cleanups wait
/// until every test is done, since another instance may still use what they made.
pub struct Sweep {
    cx: Arc<ActionCx>,
    notices: Vec<String>,
    buried: Option<Buried>,
}

/// The run's store, and the tombstones its cache sends as it replaces entries.
struct Buried {
    store: Arc<dyn CacheStore>,
    tombstones: UnboundedReceiver<CacheEntry>,
}

/// Why the cleanups of replaced entries did not run.
const ABANDONED: &str =
    "cache: interrupted, so the cleanups of the entries this run replaced run at the start of the next run";

impl Sweep {
    /// `cx` without the cache, and `notices` saying why.
    pub fn off(cx: ActionCx, notices: Vec<String>) -> Self {
        Self { cx: Arc::new(cx), notices, buried: None }
    }

    /// The context the run's actions share.
    pub fn cx(&self) -> Arc<ActionCx> {
        Arc::clone(&self.cx)
    }

    /// Runs the cleanups of the entries replaced during the run as their expiry would have,
    /// removing each tombstone once its cleanups have run; the notices of the run's start, then
    /// one per cleanup that failed. The third of the run's `interrupts` abandons what is left,
    /// and the next run cleans those tombstones up as expired entries.
    pub async fn finish(self, interrupts: &mut Interrupts) -> Vec<String> {
        let Self { cx, mut notices, buried } = self;
        let Some(mut buried) = buried else { return notices };
        let tombstones: Vec<CacheEntry> = std::iter::from_fn(|| buried.tombstones.try_recv().ok()).collect();
        if tombstones.is_empty() {
            return notices;
        }
        tokio::select! {
            biased;
            () = interrupts.abandoned() => notices.push(ABANDONED.to_owned()),
            swept = buried.sweep(&cx, tombstones) => notices.extend(swept),
        }
        notices
    }
}

impl Buried {
    /// Runs the cleanups of each tombstone, then removes it; a notice per cleanup that failed
    /// and per tombstone the store could not remove, which the next run cleans up again.
    async fn sweep(&self, cx: &Arc<ActionCx>, tombstones: Vec<CacheEntry>) -> Vec<String> {
        let cleaner = Cleaner::new(cx);
        let mut notices = Vec::new();
        for tombstone in tombstones {
            let key = tombstone.key;
            notices.extend(cleaner.clean(vec![tombstone]).await);
            if let Err(error) = self.store.remove(&key) {
                notices.push(format!("cache: {error}; a replaced entry is cleaned up again by the next run"));
            }
        }
        notices
    }
}

impl Engine {
    /// `cx` keeping `cache:`d results in the cross-run cache, once the cleanups of the entries
    /// expired at the run's start have run, with the notices of a cache that is off or failed.
    /// A program with no `cache:` action leaves the cache, and the keychain, alone.
    pub(crate) async fn cached(&self, cx: ActionCx, prepared: &Prepared) -> Sweep {
        if !crate::body_files::caches(&prepared.program) {
            return Sweep::off(cx, Vec::new());
        }
        let store = match self.cache_store(prepared.environment.as_ref()) {
            Ok(store) => store,
            Err(error) => return Sweep::off(cx, vec![off(&error)]),
        };
        let (replaced, tombstones) = mpsc::unbounded_channel();
        let cx = Arc::new(cx.with_cache(RunCache::new(Arc::clone(&store), Arc::clone(&self.clock), replaced)));
        let notices = match store.take_expired(cx.values.inputs().started_ms) {
            Ok(expired) => Cleaner::new(&cx).clean(expired).await,
            Err(error) => vec![off(&store_error(&error))],
        };
        Sweep { cx, notices, buried: Some(Buried { store, tombstones }) }
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
        let cleanups = crate::body_files::cleanups(&prepared.program);
        let secrets =
            self.secrets(SecretSource::of(&prepared), &crate::ports::cleanup_reads(&prepared.program)).await?;
        let cx = self.adhoc_cx(&prepared, (&cleanups, 0, secrets)).await?;
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
        let store = SqliteCache::open(&crate::paths::under(self.root(), CACHE), &secret)
            .map_err(|error| store_error(&error))?;
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
        Arc::new(ActionCx::new(Arc::clone(program), Arc::clone(&self.cx.executors), inputs).with_auth(self.cx.auth()))
    }
}

/// Runs one stored cleanup: its api's `cleanup` with the stored vars.
async fn run_cleanup(cx: &Arc<ActionCx>, cleanup: &CachedCleanup) -> Result<(), String> {
    let request = ActionRequest::stored_cleanup(cx, cleanup)
        .ok_or_else(|| format!("`{}` no longer has a `cleanup`, so what it made is left in place", cleanup.api))?;
    let report = cx.executors.execute(request).await;
    report.outcome.failure().map_or(Ok(()), |failure| Err(failure.message.clone()))
}

/// The settings and environment of `layered` with nothing planned, for actions run outside a
/// run.
fn unplanned(layered: Layered) -> Prepared {
    let Layered { program, notices, settings, environment, dot_env, .. } = layered;
    let (plan, profiles, names) = (Arc::new(Plan::default()), Vec::new(), Vec::new());
    Prepared { program, plan, settings, profiles, names, notices, environment, dot_env, draw: Draw::new(None) }
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
