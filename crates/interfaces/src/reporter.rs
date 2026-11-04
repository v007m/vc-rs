use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use vc_model::{Event, RunStatus, Totals};

/// Turns a run's events into output: console lines, a CTRF or `JUnit` file, an NDJSON stream.
///
/// Reporters are subscribers of the event bus and receive events after the redactor. Each one
/// runs in its own task, so `&mut self` needs no lock. They are chosen by name from the
/// reporters adapter's registry (D43); `vc-testkit` has a recording fake, and every
/// implementation runs `reporter_suite` (D112).
#[async_trait]
pub trait Reporter: Send {
    /// Takes the next event, in run order. Events may name IDs the reporter cannot resolve; it
    /// reports them by their raw form instead of failing.
    ///
    /// # Errors
    ///
    /// [`ReportError::Write`] when output cannot be written.
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError>;

    /// Ends the report and returns the files it wrote, none for a console reporter.
    ///
    /// # Errors
    ///
    /// [`ReportError::Write`] or [`ReportError::Encode`] when the report cannot be produced.
    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError>;
}

/// How a run ended, handed to [`Reporter::finish`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "run result built and read across crates; every user is in this repo (D160)"
)]
pub struct RunSummary {
    /// How the run ended.
    pub status: RunStatus,
    /// Every instance by status.
    pub totals: Totals,
    /// When the run started, in milliseconds since the Unix epoch.
    pub started_ms: u64,
    /// The run's wall time.
    pub duration: Duration,
    /// The active profiles, in the order given.
    pub profiles: Vec<String>,
    /// Files other reporters wrote, for the console's `reports` line: finish the file
    /// reporters first.
    pub artifacts: Vec<Artifact>,
}

impl RunSummary {
    /// A run that ended with `status` and `totals`, started at the epoch, taking no time.
    #[must_use]
    pub const fn new(status: RunStatus, totals: Totals) -> Self {
        Self { status, totals, started_ms: 0, duration: Duration::ZERO, profiles: Vec::new(), artifacts: Vec::new() }
    }

    /// The same summary, started at `started_ms` (Unix milliseconds) and lasting `duration`.
    #[must_use]
    pub fn timed(self, started_ms: u64, duration: Duration) -> Self {
        Self { started_ms, duration, ..self }
    }

    /// The same summary with the active `profiles`.
    #[must_use]
    pub fn profiles(self, profiles: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { profiles: profiles.into_iter().map(Into::into).collect(), ..self }
    }

    /// The same summary listing `artifacts` other reporters wrote.
    #[must_use]
    pub fn artifacts(self, artifacts: Vec<Artifact>) -> Self {
        Self { artifacts, ..self }
    }
}

/// A file a reporter wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "report result built and read across crates; every user is in this repo (D160)"
)]
pub struct Artifact {
    /// The reporter's registered name: `ctrf`, `junit`.
    pub reporter: String,
    /// Where the file is.
    pub path: PathBuf,
}

impl Artifact {
    /// The file at `path`, written by `reporter`.
    #[must_use]
    pub fn new(reporter: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self { reporter: reporter.into(), path: path.into() }
    }
}

/// Why a report could not be produced. A report failure never changes test results.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReportError {
    /// No reporter is registered under this name.
    #[error("unknown reporter {name:?}; known reporters: {known}")]
    Unknown {
        /// The name asked for.
        name: String,
        /// The registered names, comma-separated.
        known: String,
    },
    /// Output could not be written.
    #[error("cannot write {location}: {reason}")]
    Write {
        /// The file, or `console`.
        location: String,
        /// What went wrong.
        reason: String,
    },
    /// The report could not be encoded.
    #[error("cannot encode the {reporter} report: {reason}")]
    Encode {
        /// The reporter's name.
        reporter: String,
        /// What went wrong.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vc_model::{RunStatus, Totals};

    use super::{Artifact, ReportError, RunSummary};

    #[test]
    fn a_summary_is_built_step_by_step() {
        let artifact = Artifact::new("ctrf", "out/ctrf.json");
        let summary = RunSummary::new(RunStatus::Passed, Totals::default())
            .timed(7, Duration::from_millis(30))
            .profiles(["staging", "eu"])
            .artifacts(vec![artifact.clone()]);
        assert_eq!((summary.started_ms, summary.duration), (7, Duration::from_millis(30)));
        assert_eq!(summary.profiles, ["staging", "eu"]);
        assert_eq!(summary.artifacts, [artifact]);
        assert_eq!(RunSummary::new(RunStatus::Failed, Totals::default()).duration, Duration::ZERO);
    }

    #[test]
    fn errors_name_what_failed() {
        let unknown = ReportError::Unknown { name: "tap".into(), known: "ctrf, junit".into() };
        assert_eq!(unknown.to_string(), r#"unknown reporter "tap"; known reporters: ctrf, junit"#);
        let write = ReportError::Write { location: "console".into(), reason: "broken pipe".into() };
        assert_eq!(write.to_string(), "cannot write console: broken pipe");
    }
}
