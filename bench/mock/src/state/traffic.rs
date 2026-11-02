use std::collections::{BTreeMap, HashMap};

use crate::stats::Stats;

/// Per-route request counts and in-flight gauges.
#[derive(Default)]
pub struct Traffic {
    requests: BTreeMap<String, u64>,
    in_flight: HashMap<String, u64>,
    max_in_flight: BTreeMap<String, u64>,
    total_in_flight: u64,
    max_total: u64,
}

impl Traffic {
    pub fn begin(&mut self, route: &str) {
        *self.requests.entry(route.to_owned()).or_default() += 1;
        let in_flight = self.in_flight.entry(route.to_owned()).or_default();
        *in_flight += 1;
        let max = self.max_in_flight.entry(route.to_owned()).or_default();
        *max = (*max).max(*in_flight);
        self.total_in_flight += 1;
        self.max_total = self.max_total.max(self.total_in_flight);
    }

    /// Saturates at zero, so a request that began before a reset ends cleanly.
    pub fn end(&mut self, route: &str) {
        if let Some(in_flight) = self.in_flight.get_mut(route) {
            *in_flight = in_flight.saturating_sub(1);
        }
        self.total_in_flight = self.total_in_flight.saturating_sub(1);
    }

    pub fn stats(&self) -> Stats {
        Stats {
            requests: self.requests.clone(),
            logins: 0,
            max_in_flight: self.max_in_flight.clone(),
            max_in_flight_total: self.max_total,
        }
    }
}
