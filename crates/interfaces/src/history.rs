//! The run history (`.vibranium/history.db`, D170): the writer task's port and the queries'.
//!
//! Everything handed to a [`HistoryWriter`] is already redacted and its bodies already cut at
//! `history.maxBodySize`; a writer stores what it is given and never masks or reads raw values.

use std::fmt;

use vc_model::{Keep, RunStatus, Totals};

use crate::{ApiTrend, ExchangeRow, RunFilter, TestRow, TestTrend};

/// Stores runs as they happen.
///
/// One writer task owns the store; calls only queue work for it, except
/// [`HistoryWriter::begin`], [`HistoryWriter::finish`] and [`HistoryWriter::prune`], which wait
/// for it. `vc-testkit` has a memory fake, and every implementation runs `history_writer_suite`
/// (D112).
pub trait HistoryWriter: Send + Sync {
    /// Opens a run and gives its new ID, greater than every earlier one.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be written.
    fn begin(&self, start: &RunStart) -> Result<RunId, HistoryError>;

    /// Queues results of `run`; they are committed in batches.
    ///
    /// # Errors
    ///
    /// [`HistoryError::Stopped`] when the writer is no longer running.
    fn record(&self, run: RunId, rows: Vec<Row>) -> Result<(), HistoryError>;

    /// Commits everything queued for `run` and closes it with `end`.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be written.
    fn finish(&self, run: RunId, end: RunEnd) -> Result<(), HistoryError>;

    /// Deletes the oldest runs beyond `keep` and says how many went.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be written.
    fn prune(&self, keep: Keep) -> Result<usize, HistoryError>;
}

/// Answers questions about stored runs, from its own read-only view, so it can run beside the
/// writer. Every implementation runs `history_reader_suite` (D112).
pub trait HistoryReader: Send + Sync {
    /// The newest `limit` runs that `filter` matches, newest first.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be read.
    fn runs(&self, limit: usize, filter: &RunFilter) -> Result<Vec<RunRow>, HistoryError>;

    /// The run `which` names, if it is stored.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be read.
    fn run(&self, which: RunRef) -> Result<Option<RunRow>, HistoryError>;

    /// The test results of `run`, in the order they finished, with their checks; their
    /// exchanges are left out.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be read.
    fn tests(&self, run: RunId) -> Result<Vec<TestRow>, HistoryError>;

    /// The report snapshot `run` was closed with (D167), if it was.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be read.
    fn snapshot(&self, run: RunId) -> Result<Option<Vec<u8>>, HistoryError>;

    /// How each test did over the newest `window` runs that `filter` matches (G15), of kind
    /// `run` when it names no kind, one trend per test in test ID order.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be read.
    fn trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<TestTrend>, HistoryError>;

    /// How each api did over the same runs as [`HistoryReader::trends`]: every call of it, those
    /// of shared and lifecycle actions included, summed by [`crate::api_trends`] newest call
    /// first, one trend per api in api ID order.
    ///
    /// # Errors
    ///
    /// [`HistoryError`] when the store cannot be read.
    fn api_trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<ApiTrend>, HistoryError>;
}

/// A stored run's number: 483, shown as `483` and used in `.vibranium/reports/483/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(u64);

impl RunId {
    /// The run numbered `number`.
    #[must_use]
    pub const fn new(number: u64) -> Self {
        Self(number)
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which run a query means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub enum RunRef {
    /// The newest run that is not an ad hoc call (`adhoc`): what `vc history show`,
    /// `vc report` and `vc run --failed` mean by the last run.
    Last,
    /// The run with this ID.
    Id(RunId),
}

impl RunRef {
    /// `last` or a run number, as written on the command line.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text == "last" {
            return Some(Self::Last);
        }
        text.parse().ok().map(|number| Self::Id(RunId::new(number)))
    }
}

/// What is known of a run when it starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct RunStart {
    /// `run`, `load`, `adhoc` or `agent`.
    pub kind: String,
    /// When it started, in Unix milliseconds.
    pub started_ms: u64,
    /// The active profiles, in the order given.
    pub profiles: Vec<String>,
    /// The selectors it was started with, as typed.
    pub selector: String,
    /// The `--seed` it ran with.
    pub seed: Option<u64>,
    /// The commit checked out, when known.
    pub git_commit: Option<String>,
    /// The branch checked out, when known.
    pub git_branch: Option<String>,
    /// The `vc` version.
    pub vc_version: String,
    /// A hash of `vibranium.config.yaml`.
    pub config_hash: String,
}

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct RunEnd {
    /// Its status.
    pub status: RunStatus,
    /// Every instance by status.
    pub totals: Totals,
    /// When it ended, in Unix milliseconds.
    pub finished_ms: u64,
    /// The report snapshot `vc report` rebuilds reports from (D167); opaque to the store.
    pub snapshot: Vec<u8>,
}

/// A stored run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct RunRow {
    /// Its ID.
    pub id: RunId,
    /// What was known when it started.
    pub start: RunStart,
    /// When it ended; `None` while it runs, or when its process died.
    pub finished_ms: Option<u64>,
    /// How it ended, once it has.
    pub status: Option<RunStatus>,
    /// Every instance by status, once it has ended.
    pub totals: Totals,
}

/// One result the writer stores.
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub enum Row {
    /// A test instance's result, with its checks and exchanges.
    Test(TestRow),
    /// An exchange of a lifecycle block, which belongs to no test.
    Exchange(ExchangeRow),
    /// One second of one test of a load run.
    Interval(IntervalRow),
}

/// One second of one test of a load run: its iterations and their latencies.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct IntervalRow {
    /// The test's full ID: `checkout.flow#buysOneItem`.
    pub test_id: String,
    /// The second of the run the iterations ended in, from 1.
    pub second: u64,
    /// How many iterations ended in it.
    pub count: u64,
    /// How many of them failed.
    pub errors: u64,
    /// Their latencies, as an encoded HDR histogram.
    pub histogram: Vec<u8>,
}

/// Why the history could not be used. A history failure never changes test results.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HistoryError {
    /// The store could not be opened or created.
    #[error("cannot open the history at {location}: {reason}")]
    Open {
        /// Where the store is.
        location: String,
        /// What went wrong.
        reason: String,
    },
    /// Another process kept the store locked past the busy timeout.
    #[error("the history at {location} is locked by another vc process; try again when it ends")]
    Busy {
        /// Where the store is.
        location: String,
    },
    /// A read or write failed.
    #[error("the history failed: {reason}")]
    Failed {
        /// What went wrong.
        reason: String,
    },
    /// The writer task is no longer running.
    #[error("the history writer stopped")]
    Stopped,
}

#[cfg(test)]
mod tests {
    use super::{HistoryError, RunId, RunRef};

    #[test]
    fn run_refs_are_last_or_a_number() {
        assert_eq!(RunRef::parse("last"), Some(RunRef::Last));
        assert_eq!(RunRef::parse("483"), Some(RunRef::Id(RunId::new(483))));
        assert_eq!(RunRef::parse("-1"), None);
        assert_eq!(RunRef::parse("latest"), None);
        assert_eq!((RunId::new(7).to_string(), RunId::new(7).get()), ("7".to_owned(), 7));
    }

    #[test]
    fn errors_say_what_failed() {
        let busy = HistoryError::Busy { location: ".vibranium/history.db".into() };
        assert!(busy.to_string().contains("locked by another vc process"));
        assert_eq!(HistoryError::Stopped.to_string(), "the history writer stopped");
    }
}
