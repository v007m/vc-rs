//! What thresholds are judged on: totals over the rolling window or over the whole run.

use std::collections::{BTreeMap, VecDeque};

use vc_model::TestId;

use crate::{Interval, LoadError, Tally};

/// How many seconds the rolling thresholds look back.
pub const WINDOW_SECONDS: u64 = 10;

/// How many seconds the live view's rate looks back: fewer than the window, so the rate
/// follows the users as a stage ramps them.
pub const RATE_SECONDS: u64 = 3;

/// Iterations over a span of seconds: every mix test's together and each test's own.
#[derive(Clone, Debug)]
pub struct Totals {
    seconds: u64,
    all: Tally,
    tests: BTreeMap<TestId, Tally>,
}

impl Totals {
    /// No iterations over no time.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if a histogram cannot be made.
    pub fn new() -> Result<Self, LoadError> {
        Ok(Self { seconds: 0, all: Tally::new()?, tests: BTreeMap::new() })
    }

    /// Adds `row`'s iterations; the span is set apart, with [`Totals::cover`].
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if a histogram cannot be made or merged.
    pub fn add(&mut self, row: &Interval) -> Result<(), LoadError> {
        self.all.merge(&row.tally)?;
        if let Some(tally) = self.tests.get_mut(&row.test) {
            return tally.merge(&row.tally);
        }
        self.tests.insert(row.test, row.tally.clone());
        Ok(())
    }

    /// Sets the span to `seconds`, which rates such as `rps` divide by.
    pub const fn cover(&mut self, seconds: u64) {
        self.seconds = seconds;
    }

    /// The span in seconds.
    #[must_use]
    pub const fn seconds(&self) -> u64 {
        self.seconds
    }

    /// Every mix test's iterations together.
    #[must_use]
    pub const fn all(&self) -> &Tally {
        &self.all
    }

    /// `test`'s iterations, if it ran in the span.
    #[must_use]
    pub fn test(&self, test: TestId) -> Option<&Tally> {
        self.tests.get(&test)
    }

    /// Each test that ran in the span, with its iterations, in test order.
    pub fn tests(&self) -> impl Iterator<Item = (TestId, &Tally)> {
        self.tests.iter().map(|(test, tally)| (*test, tally))
    }
}

/// The rows of the last [`WINDOW_SECONDS`] closed seconds.
#[derive(Clone, Debug, Default)]
pub struct Window {
    rows: VecDeque<Interval>,
    end: u64,
}

impl Window {
    /// An empty window at the run's start.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the window's end to `end`, the first second still open, taking in `rows` (the
    /// seconds closed since) and dropping rows older than the window.
    pub fn advance(&mut self, end: u64, rows: impl IntoIterator<Item = Interval>) {
        self.end = self.end.max(end);
        self.rows.extend(rows);
        let start = self.end.saturating_sub(WINDOW_SECONDS);
        self.rows.retain(|row| row.second >= start && row.second < self.end);
    }

    /// The window's totals, over [`WINDOW_SECONDS`] or the seconds since the start if fewer.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if a histogram cannot be made or merged.
    pub fn totals(&self) -> Result<Totals, LoadError> {
        let mut totals = Totals::new()?;
        for row in &self.rows {
            totals.add(row)?;
        }
        totals.cover(self.end.min(WINDOW_SECONDS));
        Ok(totals)
    }

    /// Whole iterations per second over the last [`RATE_SECONDS`], or the seconds since the
    /// start if fewer, rounded.
    #[must_use]
    pub fn rps(&self) -> u64 {
        let start = self.end.saturating_sub(RATE_SECONDS);
        let recent = self.rows.iter().filter(|row| row.second >= start);
        let count = recent.fold(0_u64, |count, row| count.saturating_add(row.tally.count()));
        let span = self.end.clamp(1, RATE_SECONDS);
        count.saturating_add(span / 2) / span
    }
}
