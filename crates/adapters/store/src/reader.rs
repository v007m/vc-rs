use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, Row as SqlRow, params};
use vc_interfaces::{
    ApiTrend, CheckRow, ExchangeRow, HistoryError, HistoryReader, RunFilter, RunId, RunRef, RunRow, RunStart, TestRow,
    TestTrend, TimingRow, api_trends,
};
use vc_model::{TestStatus, Totals};

use crate::codec::{Location, decompress, from_json, run_status, test_status, test_status_text, unreadable};
use crate::schema;

/// The columns a run is read from, in [`run_row`]'s order.
const RUN_COLUMNS: &str = "id, started_at, finished_at, profiles, selector, git_commit, git_branch, seed, vc_version,
    config_hash, kind, status, passed, failed, blocked, skipped, flaky";

/// The kind of the runs trends are taken over when a filter names none.
const TREND_KIND: &str = "run";

/// The runs a [`RunFilter`] matches, its kind, profile and branch bound to `?2`, `?3` and `?4`; a
/// `NULL` matches every run.
const MATCHING: &str = "(?2 IS NULL OR kind = ?2)
    AND (?3 IS NULL OR EXISTS (SELECT 1 FROM json_each(runs.profiles) WHERE value = ?3))
    AND (?4 IS NULL OR git_branch = ?4)";

/// The calls of an api in the newest `?1` runs [`MATCHING`] matches, newest first: the window's run
/// IDs bound the scan, so only their exchanges are read (`exchanges_by_run`).
fn api_calls() -> String {
    format!(
        "SELECT api_id, method, status, total_ms FROM exchanges
         WHERE api_id IS NOT NULL AND run_id IN (SELECT id FROM runs WHERE {MATCHING} ORDER BY id DESC LIMIT ?1)
         ORDER BY id DESC"
    )
}

/// Reads the history at a path. Each query opens its own read-only connection, so readers run
/// beside the writer and other processes; a missing file is an empty history.
#[derive(Clone, Debug)]
pub struct SqliteHistoryReader {
    path: PathBuf,
}

impl SqliteHistoryReader {
    /// A reader of the history at `path`; nothing is opened until a query.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self { path: path.to_path_buf() }
    }

    /// Runs `query` on a fresh read-only connection, or answers the default with no file.
    fn read<T: Default>(&self, query: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, HistoryError> {
        let Some(connection) = schema::open_for_reading(&self.path)? else { return Ok(T::default()) };
        query(&connection).map_err(|error| Location::of(&self.path).error(&error))
    }
}

impl HistoryReader for SqliteHistoryReader {
    fn runs(&self, limit: usize, filter: &RunFilter) -> Result<Vec<RunRow>, HistoryError> {
        self.read(|connection| {
            let sql = format!("SELECT {RUN_COLUMNS} FROM runs WHERE {MATCHING} ORDER BY id DESC LIMIT ?1");
            let mut statement = connection.prepare(&sql)?;
            statement.query_map((limit, &filter.kind, &filter.profile, &filter.branch), run_row)?.collect()
        })
    }

    fn run(&self, which: RunRef) -> Result<Option<RunRow>, HistoryError> {
        self.read(|connection| {
            let id = match which {
                RunRef::Last => None,
                RunRef::Id(id) => Some(id.get()),
            };
            let sql = format!("SELECT {RUN_COLUMNS} FROM runs WHERE (?1 IS NULL AND kind <> 'adhoc') OR id = ?1 ORDER BY id DESC LIMIT 1");
            connection.query_row(&sql, [id], run_row).optional()
        })
    }

    fn tests(&self, run: RunId) -> Result<Vec<TestRow>, HistoryError> {
        self.read(|connection| {
            let mut checks = checks(connection, run)?;
            let mut statement = connection.prepare(
                "SELECT id, test_id, example_row, repeat_index, status, duration_ms, attempts, blocked_by, skip_reason,
                        message
                 FROM test_results WHERE run_id = ?1 ORDER BY id",
            )?;
            let tests = statement.query_map([run.get()], |row| Ok((row.get::<_, i64>(0)?, test_row(row)?)))?;
            tests
                .map(|test| test.map(|(id, test)| TestRow { checks: checks.remove(&id).unwrap_or_default(), ..test }))
                .collect()
        })
    }

    fn snapshot(&self, run: RunId) -> Result<Option<Vec<u8>>, HistoryError> {
        self.read(|connection| {
            let data: Option<Vec<u8>> = connection
                .query_row("SELECT data FROM snapshots WHERE run_id = ?1", [run.get()], |row| row.get(0))
                .optional()?;
            data.map(|data| decompress(0, &data)).transpose()
        })
    }

    fn trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<TestTrend>, HistoryError> {
        self.read(|connection| {
            let sql = format!(
                "SELECT test_id, COUNT(*), SUM(status IN (?5, ?6)), SUM(status = ?7), AVG(duration_ms)
                 FROM test_results
                 WHERE status <> ?8 AND run_id IN (SELECT id FROM runs WHERE {MATCHING} ORDER BY id DESC LIMIT ?1)
                 GROUP BY test_id ORDER BY test_id"
            );
            let mut statement = connection.prepare(&sql)?;
            let text = test_status_text;
            let kind = filter.kind.as_deref().unwrap_or(TREND_KIND);
            let parameters = params![
                window,
                kind,
                filter.profile,
                filter.branch,
                text(TestStatus::Failed),
                text(TestStatus::Blocked),
                text(TestStatus::Flaky),
                text(TestStatus::Skipped),
            ];
            statement.query_map(parameters, trend_row)?.collect()
        })
    }

    fn api_trends(&self, window: usize, filter: &RunFilter) -> Result<Vec<ApiTrend>, HistoryError> {
        let calls: Vec<ExchangeRow> = self.read(|connection| {
            let mut statement = connection.prepare(&api_calls())?;
            let kind = filter.kind.as_deref().unwrap_or(TREND_KIND);
            statement.query_map(params![window, kind, filter.profile, filter.branch], call_row)?.collect()
        })?;
        Ok(api_trends(&calls))
    }
}

/// A call as [`api_trends`] reads it: its api, method, status and total time.
fn call_row(row: &SqlRow<'_>) -> rusqlite::Result<ExchangeRow> {
    let timing = TimingRow { total_ms: row.get::<_, Option<f64>>(3)?.unwrap_or_default(), ..TimingRow::default() };
    Ok(ExchangeRow { api_id: row.get(0)?, method: row.get(1)?, status: row.get(2)?, timing, ..ExchangeRow::default() })
}

fn run_row(row: &SqlRow<'_>) -> rusqlite::Result<RunRow> {
    let start = RunStart {
        started_ms: row.get(1)?,
        profiles: from_json(3, &row.get::<_, String>(3)?)?,
        selector: row.get(4)?,
        git_commit: row.get(5)?,
        git_branch: row.get(6)?,
        seed: row.get::<_, Option<i64>>(7)?.map(i64::cast_unsigned),
        vc_version: row.get(8)?,
        config_hash: row.get(9)?,
        kind: row.get(10)?,
    };
    let status: Option<String> = row.get(11)?;
    let status = status.map(|text| run_status(&text).ok_or_else(|| unreadable(11, &text))).transpose()?;
    let count = |column| row.get::<_, Option<usize>>(column).map(Option::unwrap_or_default);
    let totals =
        Totals { passed: count(12)?, failed: count(13)?, blocked: count(14)?, skipped: count(15)?, flaky: count(16)? };
    Ok(RunRow { id: RunId::new(row.get(0)?), start, finished_ms: row.get(2)?, status, totals })
}

fn trend_row(row: &SqlRow<'_>) -> rusqlite::Result<TestTrend> {
    Ok(TestTrend {
        test_id: row.get(0)?,
        results: row.get(1)?,
        failed: row.get(2)?,
        flaky: row.get(3)?,
        mean_ms: row.get(4)?,
    })
}

fn test_row(row: &SqlRow<'_>) -> rusqlite::Result<TestRow> {
    let status: String = row.get(4)?;
    Ok(TestRow {
        test_id: row.get(1)?,
        example_row: row.get(2)?,
        repeat_index: row.get(3)?,
        status: Some(test_status(&status).ok_or_else(|| unreadable(4, &status))?),
        duration_ms: row.get(5)?,
        attempts: row.get(6)?,
        blocked_by: row.get(7)?,
        skip_reason: row.get(8)?,
        message: row.get(9)?,
        checks: Vec::new(),
        exchanges: Vec::new(),
    })
}

/// Every check of `run`'s tests, by test result, in the order they were stored.
fn checks(connection: &Connection, run: RunId) -> rusqlite::Result<BTreeMap<i64, Vec<CheckRow>>> {
    let mut statement = connection.prepare(
        "SELECT c.test_result_id, c.kind, c.path, c.expected, c.actual, c.passed, c.origin, c.file, c.line
         FROM checks c JOIN test_results t ON t.id = c.test_result_id
         WHERE t.run_id = ?1 ORDER BY c.rowid",
    )?;
    let rows = statement.query_map([run.get()], |row| {
        let check = CheckRow {
            kind: row.get(1)?,
            path: row.get(2)?,
            expected: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            actual: row.get(4)?,
            passed: row.get(5)?,
            origin: row.get(6)?,
            file: row.get(7)?,
            line: row.get(8)?,
        };
        Ok((row.get::<_, i64>(0)?, check))
    })?;
    let mut checks: BTreeMap<i64, Vec<CheckRow>> = BTreeMap::new();
    for row in rows {
        let (test, check) = row?;
        checks.entry(test).or_default().push(check);
    }
    Ok(checks)
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
