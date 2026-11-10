use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Request counters, as served by `GET /__stats`.
///
/// Routes are keyed by method and route template, for example `DELETE /users/{id}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Stats {
    /// Requests received per route.
    pub requests: BTreeMap<String, u64>,
    /// Successful `POST /auth/token` calls.
    pub logins: u64,
    /// Highest number of requests in flight at once, per route.
    pub max_in_flight: BTreeMap<String, u64>,
    /// Highest number of requests in flight at once across all routes.
    pub max_in_flight_total: u64,
}

impl Stats {
    /// Requests received for `route` (`"GET /users/{id}"`), zero when it was never called.
    #[must_use]
    pub fn count(&self, route: &str) -> u64 {
        self.requests.get(route).copied().unwrap_or_default()
    }
}
