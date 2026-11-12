//! The stored runs (D170): `vc history` lists them and shows one, `vc report` renders a
//! stored run's report files again from its snapshot (D167).

use std::fmt::{Debug, Write as _};
use std::path::PathBuf;

use serde_json::{Value, json};
use vc_adapter_reporters::RunSnapshot;
use vc_interfaces::{ApiTrend, RunFilter, RunRow, TestRow, TestTrend};
use vc_model::{Date, Program};

use crate::history::history_error;
use crate::load::LOAD_KIND;
use crate::reporting::{self, Chosen};
use crate::settings::{Named, Settings};
use crate::{Engine, EngineError, ReportFlags};

/// Milliseconds in a day, for the start times.
const DAY_MS: u64 = 86_400_000;
/// What `vc report` writes when `--format` names nothing.
const REPORT: &str = "html";

/// What `vc report` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct ReportRequest {
    /// `last` or a run number; `last` when empty.
    pub run: String,
    /// `--format` (`html` when empty), `--out` (over `.vibranium/reports/<run>/`) and
    /// `--single-file`.
    pub files: ReportFlags,
}

/// The report files `vc report` wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReportFiles {
    /// The run's number.
    pub run: u64,
    /// The files, the main one of each format.
    pub files: Vec<PathBuf>,
    /// Why a format wrote nothing, one line each.
    pub notices: Vec<String>,
}

/// The stored runs, newest first, as `vc history` lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryReport {
    runs: Vec<RunRow>,
}

/// One stored run and its tests, as `vc history show` prints it.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredRun {
    run: RunRow,
    tests: Vec<TestRow>,
}

impl Engine {
    /// The newest `limit` stored runs that `filter` matches.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the history cannot be read.
    pub fn history(&self, limit: usize, filter: &RunFilter) -> Result<HistoryReport, EngineError> {
        let runs = self.history_reader().runs(limit, filter).map_err(|error| history_error(&error))?;
        Ok(HistoryReport { runs })
    }

    /// How each test did over the newest `window` runs that `filter` matches, of kind `run`
    /// when it names none (G15), for the History and Insights screens.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the history cannot be read.
    pub fn trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<TestTrend>, EngineError> {
        self.history_reader().trends(window, filter).map_err(|error| history_error(&error))
    }

    /// How each api did over the same runs as [`Engine::trends`], shared and lifecycle calls
    /// included: its calls, p50 and p95 and statuses, for Insights' slowest apis.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the history cannot be read.
    pub fn api_trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<ApiTrend>, EngineError> {
        self.history_reader().api_trends(window, filter).map_err(|error| history_error(&error))
    }

    /// The stored run `which` names, `last` or a number, with its tests.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when no such run is stored or the history cannot be read.
    pub fn stored(&self, which: &str) -> Result<StoredRun, EngineError> {
        let run = self.stored_run(which)?;
        let tests = self.history_reader().tests(run.id).map_err(|error| history_error(&error))?;
        Ok(StoredRun { run, tests })
    }

    /// Writes the report files `request` names for a stored run, from its snapshot.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the run is not stored, is a load run, has no snapshot or
    /// the history cannot be read, and [`EngineError::Reporter`] for a name that is not a report format.
    pub fn report(&self, request: &ReportRequest) -> Result<ReportFiles, EngineError> {
        let which = if request.run.is_empty() { "last" } else { request.run.as_str() };
        let row = self.stored_run(which)?;
        if row.start.kind == LOAD_KIND {
            return Err(load_run(&row));
        }
        let snapshot = self.snapshot(&row)?;
        let chosen = report_choice(&request.files)?.under(self.root()).flat(request.files.out.is_some());
        let (artifacts, notices) = chosen.render(&snapshot, row.id)?;
        let files = artifacts.into_iter().map(|artifact| artifact.path).collect();
        Ok(ReportFiles { run: row.id.get(), files, notices })
    }

    /// The report snapshot stored with `row`.
    fn snapshot(&self, row: &RunRow) -> Result<RunSnapshot, EngineError> {
        let stored = self.history_reader().snapshot(row.id).map_err(|error| history_error(&error))?;
        let bytes = stored.ok_or_else(|| EngineError::History {
            reason: format!("run {} has no report snapshot", row.id),
            help: Some("a run stores one when it finishes; this one did not".to_owned()),
        })?;
        RunSnapshot::from_json(&bytes).map_err(|error| EngineError::History {
            reason: format!("run {}'s report snapshot cannot be read: {error}", row.id),
            help: Some("it was stored by another `vc` version; run the tests again".to_owned()),
        })
    }
}

/// How to report a test run instead.
const TEST_RUN: &str = "to report a test run, pass its number from `vc history`";

/// The refusal of a report for a load run, which stores no snapshot to write files from.
fn load_run(row: &RunRow) -> EngineError {
    let scenario = &row.start.selector;
    let help = format!(
        "`vc history` lists it; for a load report, add `--json` to the next `vc load {scenario}`, \
         which sends the load again; {TEST_RUN}"
    );
    EngineError::History {
        reason: format!("run {} is a load run, and `vc report` writes the files of test runs only", row.id),
        help: Some(help),
    }
}

/// The formats and folder `flags` ask for, checked as `--format` names.
fn report_choice(flags: &ReportFlags) -> Result<Chosen, EngineError> {
    let mut names: Vec<String> =
        flags.formats.iter().flat_map(|names| names.split(',')).map(|name| name.trim().to_owned()).collect();
    names.retain(|name| !name.is_empty());
    if names.is_empty() {
        names.push(REPORT.to_owned());
    }
    let settings = Settings {
        formats: Some(Named { names, origin: "--format".to_owned() }),
        reports_dir: flags.out.as_ref().map(|out| out.to_string_lossy().into_owned()),
        html_single_file: Some(flags.single_file),
        ..Settings::default()
    };
    reporting::choose(None, &settings, &Program::default())
}

impl HistoryReport {
    /// The runs, newest first, for a screen that shows them its own way.
    #[must_use]
    pub fn runs(&self) -> &[RunRow] {
        &self.runs
    }

    /// One line per run: number, start, status, totals, profiles and selectors.
    #[must_use]
    pub fn text(&self) -> String {
        if self.runs.is_empty() {
            return "no run is stored yet\n".to_owned();
        }
        let mut out = String::new();
        for run in &self.runs {
            let _ = writeln!(out, "{}", run_line(run));
        }
        out
    }

    /// The runs as JSON, for scripts and agents.
    #[must_use]
    pub fn json(&self) -> Value {
        Value::Array(self.runs.iter().map(run_json).collect())
    }
}

impl StoredRun {
    /// The run's row.
    #[must_use]
    pub const fn run(&self) -> &RunRow {
        &self.run
    }

    /// Every test of the run, in the order they were stored.
    #[must_use]
    pub fn tests(&self) -> &[TestRow] {
        &self.tests
    }

    /// The run's line, then a line per test that did not pass.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = format!("{}\n", run_line(&self.run));
        let shown = self.tests.iter().filter(|test| text(test.status.as_ref()) != "passed");
        for test in shown {
            let _ = writeln!(out, "  {:<8} {}", text(test.status.as_ref()), instance_label(test));
            if let Some(why) = test.message.as_ref().or(test.blocked_by.as_ref()).or(test.skip_reason.as_ref()) {
                let _ = writeln!(out, "           {why}");
            }
        }
        out
    }

    /// The run and every test as JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        let tests: Vec<Value> = self.tests.iter().map(test_json).collect();
        json!({ "run": run_json(&self.run), "tests": tests })
    }
}

fn run_line(run: &RunRow) -> String {
    let totals = &run.totals;
    let counts = if run.start.kind == LOAD_KIND {
        format!("load · {} iterations · {} errors", totals.passed.saturating_add(totals.failed), totals.failed)
    } else {
        format!(
            "{} passed · {} failed · {} blocked · {} skipped",
            totals.passed, totals.failed, totals.blocked, totals.skipped
        )
    };
    let mut line = format!("{:>5}  {}  {:<9}  {counts}", run.id.get(), started(run.start.started_ms), status(run));
    if !run.start.profiles.is_empty() {
        let _ = write!(line, "  profile {}", run.start.profiles.join(","));
    }
    if !run.start.selector.is_empty() {
        let _ = write!(line, "  {}", run.start.selector);
    }
    line
}

fn run_json(run: &RunRow) -> Value {
    let totals = &run.totals;
    json!({
        "id": run.id.get(),
        "kind": run.start.kind,
        "startedMs": run.start.started_ms,
        "finishedMs": run.finished_ms,
        "status": status(run),
        "totals": { "passed": totals.passed, "failed": totals.failed, "blocked": totals.blocked,
                    "flaky": totals.flaky, "skipped": totals.skipped, "allowedFailures": totals.allowed_failures },
        "profiles": run.start.profiles,
        "selector": run.start.selector,
        "seed": run.start.seed,
        "vcVersion": run.start.vc_version,
    })
}

fn test_json(test: &TestRow) -> Value {
    json!({
        "id": instance_label(test),
        "status": text(test.status.as_ref()),
        "durationMs": test.duration_ms,
        "attempts": test.attempts,
        "message": test.message,
        "blockedBy": test.blocked_by,
        "skipReason": test.skip_reason,
    })
}

/// A run's status in lower case; `running` until it ends.
fn status(run: &RunRow) -> String {
    if run.finished_ms.is_none() { "running".to_owned() } else { text(run.status.as_ref()) }
}

/// A status enum's name in lower case.
fn text(status: Option<&impl Debug>) -> String {
    status.map_or_else(String::new, |status| format!("{status:?}").to_lowercase())
}

/// A stored test instance as `vc history show` names it: `users.crud#listsUsers [row 2]
/// (repeat 3)`, both numbered from 1 like `vc run`'s output (the `:n` selector is not known
/// here).
#[must_use]
pub fn instance_label(test: &TestRow) -> String {
    let mut id = test.test_id.clone();
    if let Some(row) = test.example_row {
        let _ = write!(id, " [row {}]", row.saturating_add(1));
    }
    if test.repeat_index > 0 {
        let _ = write!(id, " (repeat {})", test.repeat_index.saturating_add(1));
    }
    id
}

/// A Unix time in milliseconds as `2026-11-05 09:30Z`.
#[must_use]
pub fn started(ms: u64) -> String {
    let days = i64::try_from(ms / DAY_MS).unwrap_or(i64::MAX);
    let minutes = (ms % DAY_MS) / 60_000;
    let date = Date::from_unix_days(days).map_or_else(String::new, |date| date.to_string());
    format!("{date} {:02}:{:02}Z", minutes / 60, minutes % 60)
}

#[cfg(test)]
#[path = "stored_tests.rs"]
mod tests;
