//! Per-second recording: each test's iterations, errors and latencies, one row per second.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::Duration;

use vc_model::TestId;

use crate::{Latencies, LoadError};

/// Iterations over some span: how many ran, how many failed and how long each took.
#[derive(Clone, Debug)]
pub struct Tally {
    count: u64,
    errors: u64,
    latencies: Latencies,
}

impl Tally {
    /// No iterations yet.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if the latency histogram cannot be made.
    pub fn new() -> Result<Self, LoadError> {
        Ok(Self { count: 0, errors: 0, latencies: Latencies::new()? })
    }

    /// Records one iteration that took `latency` and failed or not.
    pub fn record(&mut self, latency: Duration, failed: bool) {
        self.count = self.count.saturating_add(1);
        self.errors = self.errors.saturating_add(u64::from(failed));
        self.latencies.record(latency);
    }

    /// Adds every iteration of `other`.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if the latencies cannot be merged.
    pub fn merge(&mut self, other: &Self) -> Result<(), LoadError> {
        self.count = self.count.saturating_add(other.count);
        self.errors = self.errors.saturating_add(other.errors);
        self.latencies.merge(&other.latencies)
    }

    /// How many iterations ran.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// How many of them failed.
    #[must_use]
    pub const fn errors(&self) -> u64 {
        self.errors
    }

    /// How long they took.
    #[must_use]
    pub const fn latencies(&self) -> &Latencies {
        &self.latencies
    }
}

/// One finished iteration of a mix test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sample {
    /// The test it ran.
    pub test: TestId,
    /// The second of the run it ended in, counting from 0 at the first stage's start.
    pub second: u64,
    /// From its scheduled start to its end, so a tool that falls behind cannot hide a slow
    /// server (coordinated omission).
    pub latency: Duration,
    /// Whether it failed.
    pub failed: bool,
}

/// One test's iterations in one second: a `load_intervals` row.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Interval {
    /// The test.
    pub test: TestId,
    /// The second of the run.
    pub second: u64,
    /// Its iterations.
    pub tally: Tally,
}

/// Collects samples into seconds and hands each second over once it closes.
///
/// A sample for a second already closed, held up on its way, counts in the oldest open one.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    open: BTreeMap<u64, BTreeMap<TestId, Tally>>,
    closed_before: u64,
}

impl Recorder {
    /// No samples and no closed seconds.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `sample` in its second.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if the second's histogram cannot be made.
    pub fn record(&mut self, sample: Sample) -> Result<(), LoadError> {
        let second = sample.second.max(self.closed_before);
        let tally = match self.open.entry(second).or_default().entry(sample.test) {
            Entry::Occupied(tally) => tally.into_mut(),
            Entry::Vacant(slot) => slot.insert(Tally::new()?),
        };
        tally.record(sample.latency, sample.failed);
        Ok(())
    }

    /// Closes every second before `second` and returns their rows, by second and then test.
    pub fn close(&mut self, second: u64) -> Vec<Interval> {
        let still_open = self.open.split_off(&second);
        let closed = std::mem::replace(&mut self.open, still_open);
        self.closed_before = self.closed_before.max(second);
        closed
            .into_iter()
            .flat_map(|(second, tests)| tests.into_iter().map(move |(test, tally)| Interval { test, second, tally }))
            .collect()
    }
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
