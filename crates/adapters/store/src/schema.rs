use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use vc_interfaces::HistoryError;

use crate::codec::{Location, now_ms};

/// How long a connection waits for another process's write lock before it gives up.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The schema, one numbered step per entry; a step never changes once released.
const MIGRATIONS: &[&str] = &[r"
CREATE TABLE runs (
  id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL, finished_at INTEGER,
  profiles TEXT NOT NULL, selector TEXT NOT NULL, git_commit TEXT, git_branch TEXT,
  seed INTEGER, vc_version TEXT NOT NULL, config_hash TEXT NOT NULL,
  passed INTEGER, failed INTEGER, blocked INTEGER, skipped INTEGER, flaky INTEGER,
  kind TEXT NOT NULL, status TEXT
);
CREATE TABLE test_results (
  id INTEGER PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  test_id TEXT NOT NULL, example_row INTEGER, repeat_index INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL, duration_ms REAL NOT NULL, attempts INTEGER NOT NULL DEFAULT 1,
  blocked_by TEXT, skip_reason TEXT, skip_until TEXT, message TEXT
);
CREATE TABLE checks (
  test_result_id INTEGER NOT NULL REFERENCES test_results(id) ON DELETE CASCADE,
  kind TEXT NOT NULL, path TEXT, expected TEXT, actual TEXT, passed INTEGER NOT NULL,
  origin TEXT NOT NULL, file TEXT NOT NULL, line INTEGER NOT NULL
);
CREATE TABLE exchanges (
  id INTEGER PRIMARY KEY, test_result_id INTEGER REFERENCES test_results(id) ON DELETE CASCADE,
  run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  action TEXT NOT NULL, api_id TEXT, method TEXT NOT NULL, url TEXT NOT NULL,
  status INTEGER, dns_ms REAL, connect_ms REAL, tls_ms REAL, first_byte_ms REAL, download_ms REAL, total_ms REAL,
  request_headers TEXT, response_headers TEXT, request_body BLOB, response_body BLOB, body_truncated INTEGER
);
CREATE TABLE load_intervals (
  run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE, test_id TEXT NOT NULL, second INTEGER NOT NULL,
  count INTEGER NOT NULL, errors INTEGER NOT NULL, histogram BLOB NOT NULL
);
CREATE TABLE agent_actions (
  id INTEGER PRIMARY KEY, at INTEGER NOT NULL, agent TEXT NOT NULL, tool TEXT NOT NULL,
  profile TEXT, arguments TEXT NOT NULL, result TEXT NOT NULL, approved_by TEXT
);
CREATE TABLE snapshots (
  run_id INTEGER PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE, data BLOB NOT NULL
);
CREATE INDEX test_results_by_test ON test_results(test_id, run_id);
CREATE INDEX test_results_by_run ON test_results(run_id);
CREATE INDEX exchanges_by_run ON exchanges(run_id);
CREATE INDEX runs_by_time ON runs(started_at);
"];

/// Opens (creating it if needed) the history at `path` for writing, in WAL mode, migrated.
pub fn open_for_writing(path: &Path) -> Result<Connection, HistoryError> {
    let location = Location::of(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| location.open(&error))?;
    }
    let mut connection = Connection::open(path).map_err(|error| location.open(&error))?;
    configure(&connection).map_err(|error| location.open(&error))?;
    migrate(&mut connection).map_err(|error| location.error(&error))?;
    Ok(connection)
}

/// Opens the history at `path` read-only, or `None` when there is none yet.
pub fn open_for_reading(path: &Path) -> Result<Option<Connection>, HistoryError> {
    if !path.exists() {
        return Ok(None);
    }
    let location = Location::of(path);
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags).map_err(|error| location.open(&error))?;
    connection.busy_timeout(BUSY_TIMEOUT).map_err(|error| location.open(&error))?;
    Ok(Some(connection))
}

fn configure(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")
}

/// Applies every step the file has not had yet, each in its own transaction.
fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL)",
    )?;
    let applied: u64 =
        connection.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |row| row.get(0))?;
    for (version, step) in (1_u64..).zip(MIGRATIONS).skip(usize::try_from(applied).unwrap_or(usize::MAX)) {
        let transaction = connection.transaction()?;
        transaction.execute_batch(step)?;
        transaction.execute("INSERT INTO schema_migrations VALUES (?1, ?2)", (version, now_ms()))?;
        transaction.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MIGRATIONS, open_for_reading, open_for_writing};

    #[test]
    fn a_new_file_gets_every_migration_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".vibranium/history.db");
        drop(open_for_writing(&path).unwrap());
        let connection = open_for_writing(&path).unwrap();
        let versions: u64 =
            connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0)).unwrap();
        assert_eq!(versions, u64::try_from(MIGRATIONS.len()).unwrap());
        let mode: String = connection.query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn a_missing_file_is_not_created_by_a_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        assert!(open_for_reading(&path).unwrap().is_none());
        assert!(!path.exists());
    }
}
