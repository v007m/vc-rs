use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tempfile::TempDir;
use vc_interfaces::{
    ExchangeRow, FindingKind, FindingRow, HistoryReader, HistoryWriter, IntervalRow, Row, RunEnd, RunFilter, RunId,
    RunStart, TestRow,
};
use vc_model::{RunStatus, TestStatus, Totals};
use vc_testkit::{history_reader_suite, history_writer_suite};

use super::SqliteHistory;
use crate::SqliteHistoryReader;

/// The two halves of one history.
type Halves = (Box<dyn HistoryWriter>, Box<dyn HistoryReader>);

/// A writer and a reader of a fresh history in a directory `dirs` keeps alive.
fn fresh(dirs: &mut Vec<TempDir>) -> Halves {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".vibranium/history.db");
    dirs.push(dir);
    (Box::new(SqliteHistory::open(&path).unwrap()), Box::new(SqliteHistoryReader::new(&path)))
}

#[test]
fn passes_the_history_writer_suite() {
    let mut dirs = Vec::new();
    history_writer_suite(&mut || fresh(&mut dirs));
}

#[test]
fn passes_the_history_reader_suite() {
    let mut dirs = Vec::new();
    history_reader_suite(&mut || fresh(&mut dirs));
}

/// A stored load interval: run, test, second, count, errors and histogram.
type Stored = (i64, String, i64, i64, i64, Vec<u8>);

fn passed(test_id: &str) -> Row {
    Row::Test(TestRow { test_id: test_id.into(), status: Some(TestStatus::Passed), attempts: 1, ..TestRow::default() })
}

fn count(path: &Path, table: &str) -> u64 {
    let connection = Connection::open(path).unwrap();
    connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap()
}

#[test]
fn queued_rows_are_committed_within_the_batch_wait() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let writer = SqliteHistory::open(&path).unwrap();
    let run = writer.begin(&RunStart::default()).unwrap();
    writer.record(run, vec![passed("a#b")]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while count(&path, "test_results") == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(count(&path, "test_results"), 1, "committed without a finish");
}

#[test]
fn ten_thousand_results_are_stored_in_under_a_second() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let writer = SqliteHistory::open(&path).unwrap();
    let run = writer.begin(&RunStart::default()).unwrap();
    let started = Instant::now();
    for batch in 0..100 {
        writer.record(run, (0..100).map(|n| passed(&format!("files.f{batch}#t{n}"))).collect()).unwrap();
    }
    let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
    writer.finish(run, end).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1), "took {:?}", started.elapsed());
    assert_eq!(count(&path, "test_results"), 10_000);
}

#[test]
fn bodies_are_stored_compressed_and_lifecycle_exchanges_belong_to_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let writer = SqliteHistory::open(&path).unwrap();
    let run = writer.begin(&RunStart::default()).unwrap();
    let body = b"{\"name\":\"Ada\"}".repeat(100);
    let exchange = ExchangeRow { action: "seed".into(), response_body: Some(body.clone()), ..ExchangeRow::default() };
    writer.record(run, vec![Row::Exchange(exchange)]).unwrap();
    let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
    writer.finish(run, end).unwrap();
    let connection = Connection::open(&path).unwrap();
    let (stored, owner): (Vec<u8>, Option<i64>) = connection
        .query_row("SELECT response_body, test_result_id FROM exchanges", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap();
    assert!(stored.len() < body.len() / 4, "zstd shrinks a repetitive body");
    assert_eq!(zstd::stream::decode_all(stored.as_slice()).unwrap(), body);
    assert_eq!(owner, None);
    assert_eq!(writer.prune(vc_model::Keep::Runs(0)).unwrap(), 1);
    assert_eq!(count(&path, "exchanges"), 0, "a pruned run's lifecycle exchanges go with it");
}

#[test]
fn a_load_interval_is_stored_with_its_histogram() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let writer = SqliteHistory::open(&path).unwrap();
    let run = writer.begin(&RunStart { kind: "load".into(), ..RunStart::default() }).unwrap();
    let interval =
        IntervalRow { test_id: "a.f#browses".into(), second: 3, count: 40, errors: 2, histogram: vec![1, 2, 3] };
    writer.record(run, vec![Row::Interval(interval)]).unwrap();
    let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
    writer.finish(run, end).unwrap();
    let connection = Connection::open(&path).unwrap();
    let stored: Stored = connection
        .query_row("SELECT run_id, test_id, second, count, errors, histogram FROM load_intervals", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        })
        .unwrap();
    assert_eq!(stored, (run.get().cast_signed(), "a.f#browses".to_owned(), 3, 40, 2, vec![1, 2, 3]));
}

/// A stored sample: its URL, run, test result and its finding's kind.
type Sampled = (String, i64, Option<i64>, String);

#[test]
fn an_error_kind_s_samples_are_the_run_s_exchanges_under_its_finding_and_go_with_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let writer = SqliteHistory::open(&path).unwrap();
    let run = writer.begin(&RunStart { kind: "load".into(), ..RunStart::default() }).unwrap();
    let sample = |url: &str| ExchangeRow { action: "getUser".into(), url: url.into(), ..ExchangeRow::default() };
    let breach = FindingRow { shown: "breached errorRate".into(), ..FindingRow::default() };
    let error = FindingRow {
        kind: FindingKind::Error,
        shown: "errors users#get 503 V5101 × 2".into(),
        samples: vec![sample("http://x/users/1"), sample("http://x/users/2")],
        ..FindingRow::default()
    };
    writer.record(run, vec![Row::Finding(breach), Row::Finding(error)]).unwrap();
    let end = RunEnd { status: RunStatus::Failed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
    writer.finish(run, end).unwrap();
    let connection = Connection::open(&path).unwrap();
    let mut statement = connection
        .prepare(
            "SELECT e.url, e.run_id, e.test_result_id, f.kind FROM exchanges e
             JOIN load_findings f ON f.id = e.load_finding_id ORDER BY e.id",
        )
        .unwrap();
    let stored: Vec<Sampled> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let ours = |url: &str| (url.to_owned(), run.get().cast_signed(), None, "error".to_owned());
    assert_eq!(stored, [ours("http://x/users/1"), ours("http://x/users/2")]);
    drop(statement);
    assert_eq!(writer.prune(vc_model::Keep::Runs(0)).unwrap(), 1);
    assert_eq!((count(&path, "load_findings"), count(&path, "exchanges")), (0, 0), "a pruned run's samples go with it");
}

#[test]
fn a_record_for_an_unknown_run_fails_the_next_finish() {
    let dir = tempfile::tempdir().unwrap();
    let writer = SqliteHistory::open(&dir.path().join("history.db")).unwrap();
    let run = writer.begin(&RunStart::default()).unwrap();
    writer.record(RunId::new(run.get() + 40), vec![passed("a#b")]).unwrap();
    let end = RunEnd { status: RunStatus::Passed, totals: Totals::default(), finished_ms: 1, snapshot: Vec::new() };
    assert!(writer.finish(run, end.clone()).is_err(), "a lost batch is reported");
    assert_eq!(writer.finish(run, end), Ok(()), "and only once");
}

#[test]
fn a_file_that_is_not_a_database_cannot_be_opened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    std::fs::write(&path, b"not sqlite at all, but long enough to have a header of sorts......").unwrap();
    let error = SqliteHistory::open(&path).unwrap_err();
    assert!(error.to_string().contains("history.db"), "{error}");
    assert!(SqliteHistoryReader::new(&path).runs(1, &RunFilter::default()).is_err());
}
