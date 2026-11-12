use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use vc_interfaces::HistoryError;

use crate::codec::{Location, now_ms};

/// How long a connection waits for another process's write lock before it gives up.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The schema, one numbered step per entry; a step never changes once released.
const MIGRATIONS: &[&str] = &[
    r"
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
",
    r"
CREATE TABLE approvals (
  id INTEGER PRIMARY KEY, created_at INTEGER NOT NULL, agent TEXT NOT NULL, tool TEXT NOT NULL,
  profile TEXT NOT NULL, methods TEXT NOT NULL, summary TEXT NOT NULL, arguments TEXT NOT NULL,
  status TEXT NOT NULL, decided_at INTEGER, decided_by TEXT
);
CREATE INDEX approvals_by_status ON approvals(status, id);
CREATE TABLE approval_rules (
  profile TEXT NOT NULL, tool TEXT NOT NULL, method TEXT NOT NULL,
  created_at INTEGER NOT NULL, created_by TEXT NOT NULL,
  PRIMARY KEY (profile, tool, method)
);
",
    "ALTER TABLE runs ADD COLUMN allowed_failures INTEGER;",
    "ALTER TABLE test_results ADD COLUMN label TEXT;",
];

/// Why a file could not be opened: before its migrations, or while applying them.
pub enum OpenError {
    /// The folder, the file or its settings.
    Open(String),
    /// A migration step.
    Migrate(rusqlite::Error),
}

impl OpenError {
    /// What went wrong, for an error message.
    pub fn reason(&self) -> String {
        match self {
            Self::Open(reason) => reason.clone(),
            Self::Migrate(error) => error.to_string(),
        }
    }
}

/// Opens (creating it and its folder if needed) the SQLite file at `path` in WAL mode, with
/// every step of `migrations` it has not had yet applied.
pub fn open_migrated(path: &Path, migrations: &[&str]) -> Result<Connection, OpenError> {
    let open = |error: &dyn std::fmt::Display| OpenError::Open(error.to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| open(&error))?;
    }
    let mut connection = Connection::open(path).map_err(|error| open(&error))?;
    configure(&connection).map_err(|error| open(&error))?;
    migrate(&mut connection, migrations).map_err(OpenError::Migrate)?;
    Ok(connection)
}

/// Opens (creating it if needed) the history at `path` for writing, in WAL mode, migrated.
pub fn open_for_writing(path: &Path) -> Result<Connection, HistoryError> {
    let location = Location::of(path);
    open_migrated(path, MIGRATIONS).map_err(|error| match error {
        OpenError::Open(reason) => location.open(&reason),
        OpenError::Migrate(error) => location.error(&error),
    })
}

/// Opens the history at `path` read-only, or `None` when there is none yet. A file an older `vc`
/// wrote gets its missing migrations first, so its runs read with today's columns; one that
/// cannot be written (a read-only disk, or a lock held too long) is read as it is, a count it
/// has no column for reading as none.
pub fn open_for_reading(path: &Path) -> Result<Option<Connection>, HistoryError> {
    if !path.exists() {
        return Ok(None);
    }
    let location = Location::of(path);
    let connection = read_only(path, &location)?;
    if applied(&connection).map_err(|error| location.error(&error))? < MIGRATIONS.len() {
        drop(connection);
        drop(open_for_writing(path));
        let connection = read_only(path, &location)?;
        older_columns(&connection).map_err(|error| location.error(&error))?;
        return Ok(Some(connection));
    }
    Ok(Some(connection))
}

/// Shadows each table that lacks a column a later step added (`runs.allowed_failures`, D248;
/// `test_results.label`, D316) with a temporary view that adds it as `NULL`, so the reader's
/// columns hold for a file not brought up to date.
fn older_columns(connection: &Connection) -> rusqlite::Result<()> {
    for (table, column) in [("runs", "allowed_failures"), ("test_results", "label")] {
        let has: bool = connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
            (table, column),
            |row| row.get(0),
        )?;
        if !has {
            connection
                .execute_batch(&format!("CREATE TEMP VIEW {table} AS SELECT *, NULL AS {column} FROM main.{table}"))?;
        }
    }
    Ok(())
}

fn read_only(path: &Path, location: &Location) -> Result<Connection, HistoryError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags).map_err(|error| location.open(&error))?;
    connection.busy_timeout(BUSY_TIMEOUT).map_err(|error| location.open(&error))?;
    Ok(connection)
}

/// How many migration steps the file has had; none when it has no migration table yet.
fn applied(connection: &Connection) -> rusqlite::Result<usize> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0))
}

fn configure(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(BUSY_TIMEOUT)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")
}

/// Applies every step of `migrations` the file has not had yet. The version is read and the
/// steps applied under one write lock, so two processes opening an older file at once never
/// both apply a step.
fn migrate(connection: &mut Connection, migrations: &[&str]) -> rusqlite::Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL)",
    )?;
    let applied: u64 =
        transaction.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |row| row.get(0))?;
    for (version, step) in (1_u64..).zip(migrations).skip(usize::try_from(applied).unwrap_or(usize::MAX)) {
        transaction.execute_batch(step)?;
        transaction.execute("INSERT INTO schema_migrations VALUES (?1, ?2)", (version, now_ms()))?;
    }
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Barrier};

    use super::{MIGRATIONS, applied, open_for_reading, open_for_writing};

    /// A history file an older `vc` wrote, before the `allowed_failures` and `label` columns,
    /// with one run and one test.
    fn older_file(path: &Path) {
        let connection = super::open_migrated(path, &MIGRATIONS[..2]).ok().unwrap();
        connection
            .execute_batch(
                "INSERT INTO runs (started_at, profiles, selector, vc_version, config_hash, kind, passed)
                 VALUES (1, '[]', '', '2.0.0', 'x', 'functional', 3);
                 INSERT INTO test_results (run_id, test_id, example_row, status, duration_ms)
                 VALUES (1, 'users.crud#rejectsBadEmail', 1, 'passed', 2.0);",
            )
            .unwrap();
    }

    fn label(connection: &rusqlite::Connection) -> Option<String> {
        connection.query_row("SELECT label FROM test_results", [], |row| row.get(0)).unwrap()
    }

    fn allowed_failures(connection: &rusqlite::Connection) -> Option<i64> {
        connection.query_row("SELECT allowed_failures FROM runs", [], |row| row.get(0)).unwrap()
    }

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
    fn step_two_adds_the_approval_queue_to_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        drop(super::open_migrated(&path, &MIGRATIONS[..1]).ok().unwrap());
        let connection = open_for_writing(&path).unwrap();
        let tables: u64 = connection
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('approvals', 'approval_rules')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(tables, 2);
    }

    #[test]
    fn a_reader_brings_an_older_file_up_to_date() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        drop(super::open_migrated(&path, &MIGRATIONS[..2]).ok().unwrap());
        let connection = open_for_reading(&path).unwrap().unwrap();
        let column: u64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_table_info('runs') WHERE name = 'allowed_failures'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(column, 1);
    }

    #[test]
    fn step_four_adds_an_empty_label_to_the_tests_of_an_older_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        older_file(&path);
        let connection = open_for_writing(&path).unwrap();
        assert_eq!(label(&connection), None);
        connection.execute("UPDATE test_results SET label = 'users.crud#rejectsBadEmail › Oslo'", []).unwrap();
        assert_eq!(label(&connection).as_deref(), Some("users.crud#rejectsBadEmail › Oslo"));
    }

    #[test]
    fn applied_counts_the_steps_a_file_has_had() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let empty = rusqlite::Connection::open(&path).unwrap();
        assert_eq!(applied(&empty).unwrap(), 0);
        drop(empty);
        older_file(&path);
        assert_eq!(applied(&open_for_reading(&path).unwrap().unwrap()).unwrap(), MIGRATIONS.len());
    }

    #[test]
    fn writers_and_readers_opening_an_older_file_at_once_apply_each_step_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = Arc::new(dir.path().join("history.db"));
        older_file(&path);
        let start = Arc::new(Barrier::new(8));
        let opens: Vec<_> = (0..8)
            .map(|index| {
                let (path, start) = (Arc::clone(&path), Arc::clone(&start));
                std::thread::spawn(move || {
                    start.wait();
                    if index % 2 == 0 { open_for_writing(&path).map(drop) } else { open_for_reading(&path).map(drop) }
                })
            })
            .collect();
        for open in opens {
            assert!(open.join().unwrap().is_ok());
        }
        let connection = open_for_reading(&path).unwrap().unwrap();
        assert_eq!(applied(&connection).unwrap(), MIGRATIONS.len());
    }

    #[cfg(unix)]
    #[test]
    fn a_reader_that_cannot_write_reads_an_older_file_as_it_is() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        older_file(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
        if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
            return;
        }
        let connection = open_for_reading(&path).unwrap().unwrap();
        assert_eq!(applied(&connection).unwrap(), 2);
        assert_eq!(allowed_failures(&connection), None);
        assert_eq!(label(&connection), None);
        let passed: i64 = connection.query_row("SELECT passed FROM runs", [], |row| row.get(0)).unwrap();
        assert_eq!(passed, 3);
    }

    #[test]
    fn a_missing_file_is_not_created_by_a_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        assert!(open_for_reading(&path).unwrap().is_none());
        assert!(!path.exists());
    }
}
