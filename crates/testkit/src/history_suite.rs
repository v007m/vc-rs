use vc_interfaces::{
    CheckRow, ExchangeRow, HistoryReader, HistoryWriter, Row, RunEnd, RunId, RunRef, RunStart, TestRow, TestTrend,
};
use vc_model::{Keep, RunStatus, TestStatus, Totals};

/// Builds an empty history: its writer, and a reader of the same store.
pub type MakeHistory<'a> = dyn FnMut() -> (Box<dyn HistoryWriter>, Box<dyn HistoryReader>) + 'a;

/// The contract every [`HistoryWriter`] meets (D112). `make` builds an empty history.
pub fn history_writer_suite(make: &mut MakeHistory<'_>) {
    numbers_runs_upwards(make);
    stores_what_it_is_given(make);
    prunes_the_oldest_runs(make);
}

/// The contract every [`HistoryReader`] meets (D112). `make` builds an empty history.
pub fn history_reader_suite(make: &mut MakeHistory<'_>) {
    knows_nothing_at_first(make);
    lists_the_newest_runs_first(make);
    leaves_exchanges_out_of_tests(make);
    sums_up_each_test_over_the_newest_runs(make);
}

/// A run started with `profiles`, its seed past `i64::MAX` as half of all seeds are.
fn start(profiles: &[&str]) -> RunStart {
    RunStart {
        kind: "run".into(),
        started_ms: 1_700_000_000_000,
        profiles: profiles.iter().map(|&profile| profile.to_owned()).collect(),
        selector: "users".into(),
        seed: Some(u64::MAX - 7),
        git_commit: Some("abc123".into()),
        git_branch: Some("main".into()),
        vc_version: "2.0.0".into(),
        config_hash: "f00d".into(),
    }
}

/// A failed test with one check and one exchange.
fn failed(test_id: &str) -> TestRow {
    let check = CheckRow {
        kind: "status".into(),
        expected: "200".into(),
        actual: Some("500".into()),
        origin: "test".into(),
        file: "services/users/crud.test.yaml".into(),
        line: 12,
        ..CheckRow::default()
    };
    let exchange = ExchangeRow {
        action: "getUser".into(),
        method: "GET".into(),
        url: "https://api.test/users/7".into(),
        status: Some(500),
        response_body: Some(b"{\"error\":\"boom\"}".to_vec()),
        ..ExchangeRow::default()
    };
    TestRow {
        test_id: test_id.into(),
        status: Some(TestStatus::Failed),
        duration_ms: 12.5,
        attempts: 1,
        message: Some("expected status 200, got 500".into()),
        checks: vec![check],
        exchanges: vec![exchange],
        ..TestRow::default()
    }
}

fn end(snapshot: &[u8]) -> RunEnd {
    let totals = Totals { failed: 1, ..Totals::default() };
    RunEnd { status: RunStatus::Failed, totals, finished_ms: 1_700_000_001_000, snapshot: snapshot.to_vec() }
}

/// Opens a run, stores one failed test and closes it.
fn one_run(writer: &dyn HistoryWriter, profiles: &[&str]) -> RunId {
    let run = writer.begin(&start(profiles));
    assert!(run.is_ok(), "begin failed: {run:?}");
    let run = run.unwrap_or(RunId::new(0));
    assert_eq!(writer.record(run, vec![Row::Test(failed("users.crud#getsUser"))]), Ok(()));
    assert_eq!(writer.finish(run, end(b"snapshot")), Ok(()));
    run
}

fn numbers_runs_upwards(make: &mut MakeHistory<'_>) {
    let (writer, _reader) = make();
    let first = one_run(writer.as_ref(), &["dev"]);
    let second = one_run(writer.as_ref(), &["dev"]);
    assert!(second > first, "{second} follows {first}");
}

fn stores_what_it_is_given(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let run = one_run(writer.as_ref(), &["dev", "ci"]);
    let row = reader.run(RunRef::Id(run)).ok().flatten();
    let row = row.as_ref();
    assert_eq!(row.map(|row| &row.start), Some(&start(&["dev", "ci"])), "the start is kept as given");
    assert_eq!(row.and_then(|row| row.status), Some(RunStatus::Failed));
    assert_eq!(row.map(|row| (row.finished_ms, row.totals.failed)), Some((Some(1_700_000_001_000), 1)));
    assert_eq!(reader.snapshot(run), Ok(Some(b"snapshot".to_vec())));
    let tests = reader.tests(run).unwrap_or_default();
    let expected = TestRow { exchanges: Vec::new(), ..failed("users.crud#getsUser") };
    assert_eq!(tests, vec![expected], "the test and its checks are kept as given");
}

fn prunes_the_oldest_runs(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let runs: Vec<RunId> = (0..3).map(|_| one_run(writer.as_ref(), &["dev"])).collect();
    assert_eq!(writer.prune(Keep::All), Ok(0));
    assert_eq!(writer.prune(Keep::Runs(2)), Ok(1));
    let left: Vec<RunId> = reader.runs(10).unwrap_or_default().into_iter().map(|row| row.id).collect();
    assert_eq!(left, runs.iter().rev().take(2).copied().collect::<Vec<_>>());
    assert_eq!(reader.tests(runs[0]), Ok(Vec::new()), "a pruned run's tests go with it");
    assert_eq!(reader.snapshot(runs[0]), Ok(None));
}

fn knows_nothing_at_first(make: &mut MakeHistory<'_>) {
    let (_writer, reader) = make();
    assert_eq!(reader.runs(10), Ok(Vec::new()));
    assert_eq!(reader.run(RunRef::Last), Ok(None));
    assert_eq!(reader.run(RunRef::Id(RunId::new(1))), Ok(None));
    assert_eq!(reader.tests(RunId::new(1)), Ok(Vec::new()));
    assert_eq!(reader.snapshot(RunId::new(1)), Ok(None));
}

fn lists_the_newest_runs_first(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let runs: Vec<RunId> = (0..3).map(|_| one_run(writer.as_ref(), &["dev"])).collect();
    let listed: Vec<RunId> = reader.runs(2).unwrap_or_default().into_iter().map(|row| row.id).collect();
    assert_eq!(listed, vec![runs[2], runs[1]], "newest first, at most the limit");
    let last = reader.run(RunRef::Last).ok().flatten().map(|row| row.id);
    assert_eq!(last, Some(runs[2]));
}

fn leaves_exchanges_out_of_tests(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let open = writer.begin(&start(&["dev"])).unwrap_or(RunId::new(0));
    let lifecycle = ExchangeRow { action: "seed".into(), method: "POST".into(), ..ExchangeRow::default() };
    let rows = vec![Row::Test(failed("users.crud#a")), Row::Exchange(lifecycle), Row::Test(failed("users.crud#b"))];
    assert_eq!(writer.record(open, rows), Ok(()));
    assert_eq!(writer.finish(open, end(b"")), Ok(()));
    let tests = reader.tests(open).unwrap_or_default();
    let ids: Vec<&str> = tests.iter().map(|test| test.test_id.as_str()).collect();
    assert_eq!(ids, ["users.crud#a", "users.crud#b"], "in the order they were recorded");
    assert!(tests.iter().all(|test| test.exchanges.is_empty() && test.checks.len() == 1));
}

/// A result of `test_id` that ended `status` after `duration_ms`.
fn ended(test_id: &str, status: TestStatus, duration_ms: f64) -> Row {
    Row::Test(TestRow { test_id: test_id.into(), status: Some(status), duration_ms, attempts: 1, ..TestRow::default() })
}

fn sums_up_each_test_over_the_newest_runs(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    assert_eq!(reader.trends(5), Ok(Vec::new()));
    let stored = |kind: &str, rows: Vec<Row>| {
        let run = writer.begin(&RunStart { kind: kind.into(), ..start(&["dev"]) }).unwrap_or(RunId::new(0));
        assert_eq!(writer.record(run, rows), Ok(()));
        assert_eq!(writer.finish(run, end(b"")), Ok(()));
    };
    let (a, b) = ("users.crud#a", "users.crud#b");
    stored("run", vec![ended(a, TestStatus::Failed, 99.0)]);
    stored("run", vec![ended(a, TestStatus::Passed, 10.0), ended(b, TestStatus::Blocked, 0.0)]);
    stored("load", vec![ended(a, TestStatus::Failed, 500.0)]);
    let last =
        vec![ended(a, TestStatus::Flaky, 30.0), ended(a, TestStatus::Failed, 20.0), ended(b, TestStatus::Skipped, 0.0)];
    stored("run", last);
    let expected = vec![
        TestTrend { test_id: a.into(), results: 3, failed: 1, flaky: 1, mean_ms: 20.0 },
        TestTrend { test_id: b.into(), results: 1, failed: 1, flaky: 0, mean_ms: 0.0 },
    ];
    assert_eq!(reader.trends(2), Ok(expected), "the newest 2 `run` runs, skipped results left out");
}
