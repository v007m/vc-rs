use std::path::Path;

use rusqlite::Connection;
use vc_interfaces::{
    ApiTrend, ExchangeRow, HistoryReader, HistoryWriter, Row, RunEnd, RunFilter, RunId, RunStart, TestRow, TimingRow,
};
use vc_model::{RunStatus, TestStatus, Totals};

use super::api_calls;
use crate::schema::{MIGRATIONS, open_migrated};
use crate::{SqliteHistory, SqliteHistoryReader};

/// A passed test that called `users#get` once, answered after `total_ms`.
fn called(total_ms: f64) -> Row {
    let call = ExchangeRow {
        action: "getUser".into(),
        api_id: Some("users#get".into()),
        method: "GET".into(),
        url: "https://api.test/users/7".into(),
        status: Some(200),
        timing: TimingRow { total_ms, ..TimingRow::default() },
        ..ExchangeRow::default()
    };
    let test =
        TestRow { test_id: "users.crud#a".into(), status: Some(TestStatus::Passed), attempts: 1, ..TestRow::default() };
    Row::Test(TestRow { exchanges: vec![call], ..test })
}

#[test]
fn api_trends_read_only_the_calls_of_the_runs_in_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let writer = SqliteHistory::open(&path).unwrap();
    for at in 1..=120_u32 {
        let run = writer.begin(&RunStart { kind: "run".into(), ..RunStart::default() }).unwrap();
        writer.record(run, vec![called(f64::from(at))]).unwrap();
        let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
        writer.finish(run, end).unwrap();
    }
    let trends = SqliteHistoryReader::new(&path).api_trends(100, &RunFilter::default()).unwrap();
    let expected = ApiTrend {
        api_id: "users#get".into(),
        method: "GET".into(),
        calls: 100,
        p50_ms: Some(70.0),
        p95_ms: Some(115.0),
        statuses: vec![200],
    };
    assert_eq!(trends, [expected], "the newest 100 of 120 runs: times 21 to 120 ms");
}

#[test]
fn the_calls_query_searches_the_exchanges_by_run_instead_of_scanning_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    drop(SqliteHistory::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    let mut statement = connection.prepare(&format!("EXPLAIN QUERY PLAN {}", api_calls())).unwrap();
    let parameters = (100, "run", None::<String>, None::<String>);
    let plan: Vec<String> =
        statement.query_map(parameters, |row| row.get(3)).unwrap().collect::<Result<_, _>>().unwrap();
    let exchanges: Vec<&String> = plan.iter().filter(|step| step.contains("exchanges")).collect();
    assert!(!exchanges.is_empty() && exchanges.iter().all(|step| step.contains("exchanges_by_run")), "{plan:?}");
}

/// A history file as `vc` wrote it after its first `steps` migrations (v0.1.0 had 3), holding
/// one finished load run.
fn older_load_file(path: &Path, steps: usize) {
    let connection = open_migrated(path, &MIGRATIONS[..steps]).ok().unwrap();
    connection
        .execute_batch(
            "INSERT INTO runs (started_at, finished_at, profiles, selector, vc_version, config_hash, kind, status,
                               passed, failed)
             VALUES (1, 2, '[]', 'echo.peak', '0.1.0', 'x', 'load', 'failed', 37, 3);",
        )
        .unwrap();
}

/// The tables and indexes of the file at `path`, as each was created.
fn objects(path: &Path) -> Vec<String> {
    let connection = Connection::open(path).unwrap();
    let mut statement =
        connection.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name").unwrap();
    statement.query_map([], |row| row.get(0)).unwrap().collect::<Result<_, _>>().unwrap()
}

fn applied(path: &Path) -> usize {
    let connection = Connection::open(path).unwrap();
    connection.query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| row.get(0)).unwrap()
}

#[test]
fn an_older_file_gains_only_the_findings_table_and_reads_its_load_run_as_finding_nothing() {
    for steps in [3, 4] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        older_load_file(&path, steps);
        let before = objects(&path);
        let reader = SqliteHistoryReader::new(&path);
        assert_eq!(reader.findings(RunId::new(1)).unwrap(), Vec::new(), "after {steps} steps");
        let runs = reader.runs(10, &RunFilter::default()).unwrap();
        assert_eq!((runs.len(), runs[0].start.kind.as_str(), runs[0].totals.passed), (1, "load", 37));
        assert_eq!(applied(&path), MIGRATIONS.len());
        let after = objects(&path);
        let added: Vec<&str> = after.iter().filter(|object| !before.contains(object)).map(String::as_str).collect();
        let kept = before.iter().filter(|object| after.contains(object)).count();
        if steps == 4 {
            assert_eq!(kept, before.len() - 1, "only step 6 changed what was there, the exchanges: {after:?}");
            assert_steps_5_and_6_added(&added);
        }
    }
}

/// `added` is what steps 5 and 6 create: the sample column with its index, and the findings table with its.
fn assert_steps_5_and_6_added(added: &[&str]) {
    assert_eq!(added.len(), 4, "{added:?}");
    assert!(added[0].contains("load_finding_id INTEGER REFERENCES load_findings(id) ON DELETE SET NULL"));
    assert_eq!(added[1], "CREATE INDEX exchanges_by_finding ON exchanges(load_finding_id)");
    assert!(added[2].starts_with("CREATE TABLE load_findings ("), "{added:?}");
    assert_eq!(added[3], "CREATE INDEX load_findings_by_run ON load_findings(run_id)");
}

#[test]
fn a_file_with_findings_gains_only_the_sample_column_and_its_index_and_keeps_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    older_load_file(&path, 5);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "INSERT INTO load_findings (run_id, kind, test_id, api_id, status, code, count, shown)
             VALUES (1, 'error', 'echo.send#sends', 'echo#send', 500, 'V5101', 3, 'errors echo#send 500 V5101 × 3');
             INSERT INTO exchanges (run_id, action, method, url, total_ms) VALUES (1, 'seed', 'POST', 'http://x', 1.0);",
        )
        .unwrap();
    let before = objects(&path);
    let reader = SqliteHistoryReader::new(&path);
    let findings = reader.findings(RunId::new(1)).unwrap();
    assert_eq!((findings.len(), findings[0].count, findings[0].samples.len()), (1, Some(3), 0));
    assert_eq!(applied(&path), MIGRATIONS.len());
    let after = objects(&path);
    let added: Vec<&str> = after.iter().filter(|object| !before.contains(object)).map(String::as_str).collect();
    let gone: Vec<&str> = before.iter().filter(|object| !after.contains(object)).map(String::as_str).collect();
    assert_eq!(added.len(), 2, "the exchanges with their new column, and its index: {added:?}");
    assert!(added[0].contains("load_finding_id INTEGER REFERENCES load_findings(id) ON DELETE SET NULL"));
    assert_eq!(added[1], "CREATE INDEX exchanges_by_finding ON exchanges(load_finding_id)");
    assert_eq!(gone.len(), 1, "{gone:?}");
    assert!(gone[0].contains("exchanges") && !gone[0].contains("load_finding_id"), "{gone:?}");
    let owner: Option<i64> = Connection::open(&path)
        .unwrap()
        .query_row("SELECT load_finding_id FROM exchanges WHERE action = 'seed'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(owner, None, "an exchange stored before samples were belongs to no finding");
}

#[test]
fn a_sample_outlives_a_deleted_finding_as_belonging_to_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    older_load_file(&path, MIGRATIONS.len());
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    connection
        .execute_batch(
            "INSERT INTO load_findings (id, run_id, kind, shown) VALUES (7, 1, 'error', 'errors × 1');
             INSERT INTO exchanges (run_id, action, method, url, total_ms, load_finding_id)
             VALUES (1, 'getUser', 'GET', 'http://x', 1.0, 7);
             DELETE FROM load_findings;",
        )
        .unwrap();
    let owner: Option<i64> =
        connection.query_row("SELECT load_finding_id FROM exchanges", [], |row| row.get(0)).unwrap();
    assert_eq!(owner, None);
}

#[test]
fn the_missing_steps_up_to_the_sample_column_apply_together_or_not_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    older_load_file(&path, 3);
    Connection::open(&path)
        .unwrap()
        .execute_batch("ALTER TABLE exchanges ADD COLUMN load_finding_id INTEGER;")
        .unwrap();
    assert!(SqliteHistory::open(&path).is_err(), "step 6 cannot add its column");
    assert_eq!(applied(&path), 3, "steps 4 and 5 were rolled back with step 6");
    let objects = objects(&path);
    assert!(!objects.iter().any(|object| object.contains("load_findings")), "{objects:?}");
    let labels: u64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM pragma_table_info('test_results') WHERE name = 'label'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(labels, 0);
}

#[test]
fn the_missing_steps_apply_together_or_not_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    older_load_file(&path, 3);
    Connection::open(&path).unwrap().execute_batch("CREATE TABLE load_findings (stray INTEGER);").unwrap();
    assert!(SqliteHistory::open(&path).is_err(), "step 5 cannot create its table");
    assert_eq!(applied(&path), 3, "step 4 was rolled back with step 5");
    let labels: u64 = Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM pragma_table_info('test_results') WHERE name = 'label'", [], |row| row.get(0))
        .unwrap();
    assert_eq!(labels, 0);
}

#[cfg(unix)]
#[test]
fn a_read_only_older_file_reads_a_load_run_as_finding_nothing_and_is_left_as_it_was() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    older_load_file(&path, 4);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
        return;
    }
    let bytes = std::fs::read(&path).unwrap();
    let reader = SqliteHistoryReader::new(&path);
    assert_eq!(reader.findings(RunId::new(1)).unwrap(), Vec::new());
    assert_eq!(reader.runs(10, &RunFilter::default()).unwrap().len(), 1);
    assert_eq!(std::fs::read(&path).unwrap(), bytes, "reading wrote to a read-only file");
    assert_eq!(applied(&path), 4);
    assert!(!objects(&path).iter().any(|object| object.contains("load_findings")));
}
