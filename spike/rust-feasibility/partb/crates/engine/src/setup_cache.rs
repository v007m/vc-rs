//! Singleflight memoization of setup calls keyed by (test, scope, vars).
use std::collections::HashMap;
use std::sync::{Arc, PoisonError};

use ir::{FileId, Scope, SetupRef, TestId, Vars};
use tokio::sync::OnceCell;

use crate::executor::CallError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ScopeKey {
    File(FileId),
    Run,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetupKey {
    test: TestId,
    scope: ScopeKey,
    vars: u64,
}

impl SetupKey {
    pub fn new(setup: SetupRef, requester_file: FileId, vars: &Vars) -> Self {
        let scope = match setup.scope {
            Scope::File => ScopeKey::File(requester_file),
            Scope::Run => ScopeKey::Run,
        };
        Self { test: setup.test, scope, vars: vars.fingerprint() }
    }
}

type Slot = Arc<OnceCell<Result<Vars, CallError>>>;

/// The first caller for a key runs the setup; concurrent callers await the same
/// result. Failures are memoized too, so a broken setup is not retried per test.
#[derive(Default)]
pub struct SetupCache {
    #[expect(clippy::disallowed_types, reason = "singleflight slot map; the lock is never held across an await")]
    slots: std::sync::Mutex<HashMap<SetupKey, Slot>>,
}

impl SetupCache {
    pub async fn get_or_run<F, Fut>(&self, key: SetupKey, run: F) -> Result<Vars, CallError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vars, CallError>>,
    {
        let slot = {
            let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(slots.entry(key).or_default())
        };
        slot.get_or_init(run).await.clone()
    }
}
