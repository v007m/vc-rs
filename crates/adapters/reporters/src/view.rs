//! The resolved, serializable shape of a run: what the file formats render and what the history
//! keeps as the run's snapshot (D167).
//!
//! Every ID is already turned into the text a reader sees, so a snapshot renders without the
//! program it came from. Exchanges are the redacted ones, cut at `history.maxBodySize`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vc_interfaces::{ReportError, RunSummary};
use vc_model::{Location, RunStatus, TestStatus, Totals};

/// The snapshot layout; a snapshot of another version is refused, not misread.
const VERSION: u32 = 1;

/// The name the reports give a run whose config has no `project`.
const DEFAULT_PROJECT: &str = "vibranium";

/// Which frame an action ran in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum OwnerView {
    Run,
    File { file: String },
    Instance { instance: u32 },
}

/// Why an action or a test failed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureView {
    pub message: String,
    pub code: Option<String>,
    pub action: Option<String>,
}

/// One evaluated `expect` check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckView {
    pub key: String,
    pub passed: bool,
    pub expected: Value,
    pub actual: Option<Value>,
    pub diff: Option<String>,
    pub error: Option<String>,
    pub code: Option<String>,
    pub origin: String,
    pub location: Option<Location>,
}

/// One finished action with its checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepView {
    pub action: String,
    pub owner: OwnerView,
    pub status: String,
    pub failure: Option<FailureView>,
    pub checks: Vec<CheckView>,
}

/// One cleanup that ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupView {
    pub action: String,
    pub owner: OwnerView,
    pub status: String,
    pub failure: Option<FailureView>,
}

/// A cleanup that never ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbandonedView {
    pub action: String,
    pub owner: OwnerView,
}

/// A teardown block that failed (D304): `afterAll` with its file, or `afterRun` with none, and
/// the masked message of its first failed action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeardownView {
    pub block: String,
    pub file: Option<String>,
    pub message: String,
}

impl TeardownView {
    /// The block as the console names it: `afterAll in users.crud`, `afterRun`.
    pub fn label(&self) -> String {
        self.file.as_ref().map_or_else(|| self.block.clone(), |file| format!("{} in {file}", self.block))
    }
}

/// Per-phase timings in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingView {
    pub dns: Option<f64>,
    pub connect: Option<f64>,
    pub tls: Option<f64>,
    pub first_byte: f64,
    pub download: f64,
    pub total: f64,
}

/// One redacted request and response; bodies are text, cut at `history.maxBodySize`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeView {
    pub action: String,
    pub api: Option<String>,
    pub method: String,
    pub url: String,
    pub status: u16,
    pub timing: TimingView,
    pub request_headers: Vec<(String, String)>,
    pub response_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub truncated: bool,
}

/// One finished test instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestView {
    pub id: String,
    pub label: String,
    /// The test's `name`; a snapshot from before names were kept has none.
    #[serde(default)]
    pub name: Option<String>,
    pub file: String,
    /// The `name` of the test's file.
    #[serde(default)]
    pub file_name: Option<String>,
    pub status: TestStatus,
    pub instance: Option<u32>,
    pub example_row: Option<u32>,
    pub repeat_index: u32,
    pub attempts: u32,
    /// When it started, in milliseconds after the run started as its first event arrived, and
    /// that start plus the duration the executor measured (D305); no start: it never ran.
    pub start_ms: Option<u64>,
    pub stop_ms: u64,
    pub location: Option<Location>,
    pub tags: Vec<String>,
    pub failure: Option<FailureView>,
    pub blocked_by: Option<String>,
    pub skip_reason: Option<String>,
    pub message: Option<String>,
    /// The failed checks or the failure as plain text, with source excerpts.
    pub detail: String,
    pub steps: Vec<StepView>,
    pub exchanges: Vec<ExchangeView>,
}

impl TestView {
    /// How long the test ran, zero when it never started.
    pub fn duration_ms(&self) -> u64 {
        self.start_ms.map_or(0, |start| self.stop_ms.saturating_sub(start))
    }

    /// The label followed by the test's `name`, as the console shows it:
    /// `users.crud#listsUsers — Lists every user`.
    pub fn titled(&self) -> String {
        self.name.as_ref().map_or_else(|| self.label.clone(), |name| format!("{} — {name}", self.label))
    }

    /// Every check of every step, in run order.
    pub fn checks(&self) -> impl Iterator<Item = &CheckView> {
        self.steps.iter().flat_map(|step| &step.checks)
    }
}

/// The run as a whole.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(clippy::exhaustive_structs, reason = "run facts the engine fills in; every user is in this repo (D160)")]
pub struct RunFacts {
    /// The history's run number; zero when the history is off.
    pub id: u64,
    pub status: RunStatus,
    pub totals: Totals,
    /// When the run started, in Unix milliseconds.
    pub started_ms: u64,
    pub duration_ms: u64,
    pub profiles: Vec<String>,
    pub seed: Option<u64>,
}

impl RunFacts {
    /// The facts of history run `id` that ended as `summary` says; no seed.
    #[must_use]
    pub fn of(id: u64, summary: &RunSummary) -> Self {
        Self {
            id,
            status: summary.status,
            totals: summary.totals,
            started_ms: summary.started_ms,
            duration_ms: crate::text::millis(summary.duration),
            profiles: summary.profiles.clone(),
            seed: None,
        }
    }
}

/// Everything a report format needs: the run, its files in run order, its tests, and the
/// lifecycle blocks and cleanups around them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunSnapshot {
    version: u32,
    pub(crate) run: RunFacts,
    /// The config's `project`; a snapshot from before it was kept has none.
    #[serde(default)]
    pub(crate) project: Option<String>,
    /// The `name` of each active profile that gives one, by its ID.
    #[serde(default)]
    pub(crate) profile_names: BTreeMap<String, String>,
    pub(crate) files: Vec<String>,
    pub(crate) tests: Vec<TestView>,
    pub(crate) lifecycle: Vec<StepView>,
    pub(crate) cleanups: Vec<CleanupView>,
    pub(crate) abandoned: Vec<AbandonedView>,
    /// The teardown blocks that failed; a snapshot from before they were kept has none.
    #[serde(default)]
    pub(crate) teardown_failures: Vec<TeardownView>,
}

/// The parts of a snapshot the recorder gathers, before the run facts are known.
#[derive(Debug, Default)]
pub struct Gathered {
    pub project: Option<String>,
    pub profile_names: BTreeMap<String, String>,
    pub files: Vec<String>,
    pub tests: Vec<TestView>,
    pub lifecycle: Vec<StepView>,
    pub cleanups: Vec<CleanupView>,
    pub abandoned: Vec<AbandonedView>,
    pub teardown_failures: Vec<TeardownView>,
}

impl RunSnapshot {
    pub(crate) fn new(run: RunFacts, gathered: Gathered) -> Self {
        let Gathered { project, profile_names, files, tests, lifecycle, cleanups, abandoned, teardown_failures } =
            gathered;
        let version = VERSION;
        Self { version, run, project, profile_names, files, tests, lifecycle, cleanups, abandoned, teardown_failures }
    }

    /// The teardown blocks that failed, as the console names them: `afterAll in users.crud`,
    /// `afterRun` (D279, D304).
    #[must_use]
    pub fn teardown_failed(&self) -> Vec<String> {
        self.teardown_failures.iter().map(TeardownView::label).collect()
    }

    /// The project's display name: the config's `project`, else `vibranium`.
    pub(crate) fn project(&self) -> &str {
        self.project.as_deref().unwrap_or(DEFAULT_PROJECT)
    }

    /// The run this snapshot is of.
    #[must_use]
    pub const fn run(&self) -> &RunFacts {
        &self.run
    }

    /// The snapshot as JSON, the form the history keeps.
    ///
    /// # Errors
    ///
    /// [`ReportError::Encode`] when it cannot be encoded.
    pub fn to_json(&self) -> Result<Vec<u8>, ReportError> {
        serde_json::to_vec(self).map_err(|error| snapshot_error(&error.to_string()))
    }

    /// A snapshot read back from [`RunSnapshot::to_json`].
    ///
    /// # Errors
    ///
    /// [`ReportError::Encode`] when the bytes are not a snapshot this `vc` can read.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ReportError> {
        let snapshot: Self = serde_json::from_slice(bytes).map_err(|error| snapshot_error(&error.to_string()))?;
        if snapshot.version != VERSION {
            let reason = format!("layout {} is not {VERSION}; re-run the tests to report them", snapshot.version);
            return Err(snapshot_error(&reason));
        }
        Ok(snapshot)
    }
}

fn snapshot_error(reason: &str) -> ReportError {
    ReportError::Encode { reporter: "snapshot".to_owned(), reason: reason.to_owned() }
}

#[cfg(test)]
mod tests {
    use vc_model::{RunStatus, Totals};

    use super::{Gathered, RunFacts, RunSnapshot};

    fn facts() -> RunFacts {
        let totals = Totals::default();
        RunFacts {
            id: 7,
            status: RunStatus::Passed,
            totals,
            started_ms: 1,
            duration_ms: 2,
            profiles: vec![],
            seed: None,
        }
    }

    #[test]
    fn a_snapshot_reads_back_as_written() {
        let snapshot = RunSnapshot::new(facts(), Gathered { files: vec!["users.crud".into()], ..Gathered::default() });
        let bytes = snapshot.to_json().unwrap();
        assert_eq!(RunSnapshot::from_json(&bytes).unwrap(), snapshot);
        assert_eq!(snapshot.run().id, 7);
    }

    #[test]
    fn another_layout_or_garbage_is_refused() {
        let mut snapshot = RunSnapshot::new(facts(), Gathered::default());
        snapshot.version = 99;
        let error = RunSnapshot::from_json(&snapshot.to_json().unwrap()).unwrap_err();
        assert!(error.to_string().contains("layout 99"), "{error}");
        assert!(RunSnapshot::from_json(b"[1,2]").is_err());
    }
}
