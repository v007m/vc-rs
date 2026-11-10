mod collections;
mod counters;
mod sessions;
mod traffic;

pub use collections::Collections;
pub use counters::Counters;
pub use sessions::Sessions;
pub use traffic::Traffic;

use crate::stats::Stats;

/// Everything the mock server remembers. Owned by the actor task in [`crate::store::Store`].
#[derive(Default)]
pub struct State {
    pub collections: Collections,
    pub sessions: Sessions,
    pub traffic: Traffic,
    pub flaky: Counters,
    pub polls: Counters,
    /// Which users spec `/openapi/users.yaml` serves: 0 for v1, 1 for v2.
    pub spec_version: usize,
}

impl State {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn stats(&self) -> Stats {
        let mut stats = self.traffic.stats();
        stats.logins = self.sessions.logins();
        stats
    }
}
