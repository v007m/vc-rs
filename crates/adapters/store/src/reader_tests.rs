use rusqlite::Connection;
use vc_interfaces::{
    ApiTrend, ExchangeRow, HistoryReader, HistoryWriter, Row, RunEnd, RunFilter, RunStart, TestRow, TimingRow,
};
use vc_model::{RunStatus, TestStatus, Totals};

use super::api_calls;
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
