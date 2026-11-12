use vc_interfaces::{
    ApiTrend, CheckRow, ExchangeRow, HistoryReader, HistoryWriter, Row, RunEnd, RunFilter, RunId, RunRef, RunStart,
    TestRow, TestTrend, TimingRow,
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
    takes_the_last_run_to_be_no_ad_hoc_call(make);
    leaves_exchanges_out_of_tests(make);
    sums_up_each_test_over_the_newest_runs(make);
    counts_only_the_runs_a_filter_matches(make);
    sums_up_each_api_over_the_newest_runs(make);
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
    let left: Vec<RunId> =
        reader.runs(10, &RunFilter::default()).unwrap_or_default().into_iter().map(|row| row.id).collect();
    assert_eq!(left, runs.iter().rev().take(2).copied().collect::<Vec<_>>());
    assert_eq!(reader.tests(runs[0]), Ok(Vec::new()), "a pruned run's tests go with it");
    assert_eq!(reader.snapshot(runs[0]), Ok(None));
}

fn knows_nothing_at_first(make: &mut MakeHistory<'_>) {
    let (_writer, reader) = make();
    assert_eq!(reader.runs(10, &RunFilter::default()), Ok(Vec::new()));
    assert_eq!(reader.run(RunRef::Last), Ok(None));
    assert_eq!(reader.run(RunRef::Id(RunId::new(1))), Ok(None));
    assert_eq!(reader.tests(RunId::new(1)), Ok(Vec::new()));
    assert_eq!(reader.snapshot(RunId::new(1)), Ok(None));
}

fn lists_the_newest_runs_first(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let runs: Vec<RunId> = (0..3).map(|_| one_run(writer.as_ref(), &["dev"])).collect();
    let listed: Vec<RunId> =
        reader.runs(2, &RunFilter::default()).unwrap_or_default().into_iter().map(|row| row.id).collect();
    assert_eq!(listed, vec![runs[2], runs[1]], "newest first, at most the limit");
    let last = reader.run(RunRef::Last).ok().flatten().map(|row| row.id);
    assert_eq!(last, Some(runs[2]));
}

fn takes_the_last_run_to_be_no_ad_hoc_call(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let tests = one_run(writer.as_ref(), &["dev"]);
    let call = writer.begin(&RunStart { kind: "adhoc".into(), ..start(&["dev"]) }).unwrap_or(RunId::new(0));
    assert_eq!(writer.finish(call, end(b"call")), Ok(()));
    let last = reader.run(RunRef::Last).ok().flatten().map(|row| row.id);
    assert_eq!(last, Some(tests), "a `vc call` is not the last run");
    let named = reader.run(RunRef::Id(call)).ok().flatten().map(|row| row.id);
    assert_eq!(named, Some(call), "it is still found by its number");
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
    assert_eq!(reader.trends(5, &RunFilter::default()), Ok(Vec::new()));
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
    assert_eq!(
        reader.trends(2, &RunFilter::default()),
        Ok(expected),
        "the newest 2 `run` runs, skipped results left out"
    );
}

/// A filter of the runs of `kind`, under `profile`, on `branch`.
fn filter(kind: Option<&str>, profile: Option<&str>, branch: Option<&str>) -> RunFilter {
    let owned = |text: Option<&str>| text.map(str::to_owned);
    RunFilter { kind: owned(kind), profile: owned(profile), branch: owned(branch) }
}

fn counts_only_the_runs_a_filter_matches(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    let stored = |start: RunStart, rows: Vec<Row>| {
        let run = writer.begin(&start).unwrap_or(RunId::new(0));
        assert_eq!(writer.record(run, rows), Ok(()));
        assert_eq!(writer.finish(run, end(b"")), Ok(()));
        run
    };
    let a = "users.crud#a";
    let staging = stored(start(&["staging", "eu"]), vec![ended(a, TestStatus::Failed, 10.0)]);
    let feature = RunStart { git_branch: Some("feat/x".into()), ..start(&["staging"]) };
    let feature = stored(feature, vec![ended(a, TestStatus::Passed, 30.0)]);
    let load =
        stored(RunStart { kind: "load".into(), ..start(&["staging"]) }, vec![ended(a, TestStatus::Failed, 90.0)]);
    stored(start(&["dev"]), vec![ended(a, TestStatus::Passed, 50.0)]);
    let ids = |limit: usize, filter: &RunFilter| -> Vec<RunId> {
        reader.runs(limit, filter).unwrap_or_default().into_iter().map(|row| row.id).collect()
    };
    assert_eq!(ids(2, &filter(None, Some("staging"), None)), vec![load, feature], "the limit counts the matches");
    assert_eq!(ids(9, &filter(Some("run"), Some("staging"), Some("main"))), vec![staging]);
    assert_eq!(ids(9, &filter(None, Some("eu"), None)), vec![staging], "any of a run's profiles");
    assert_eq!(ids(9, &filter(None, Some("prod"), None)), Vec::new());
    let trend =
        |results: u32, failed: u32, mean_ms: f64| TestTrend { test_id: a.into(), results, failed, flaky: 0, mean_ms };
    let under_staging = reader.trends(1, &filter(None, Some("staging"), None));
    assert_eq!(under_staging, Ok(vec![trend(1, 0, 30.0)]), "the window counts `run` runs only, by default");
    assert_eq!(reader.trends(9, &filter(None, None, Some("main"))), Ok(vec![trend(2, 1, 30.0)]));
    assert_eq!(reader.trends(9, &filter(Some("load"), None, None)), Ok(vec![trend(1, 1, 90.0)]));
}

/// A call of `api`, its ID and method, that got `status` after `total_ms`.
fn call((api_id, method): (&str, &str), status: Option<u16>, total_ms: f64) -> ExchangeRow {
    let timing = TimingRow { total_ms, ..TimingRow::default() };
    let (api_id, method) = (Some(api_id.to_owned()), method.to_owned());
    ExchangeRow {
        action: "call".into(),
        api_id,
        method,
        url: "https://api.test/x".into(),
        status,
        timing,
        ..ExchangeRow::default()
    }
}

fn sums_up_each_api_over_the_newest_runs(make: &mut MakeHistory<'_>) {
    let (writer, reader) = make();
    assert_eq!(reader.api_trends(5, &RunFilter::default()), Ok(Vec::new()));
    let stored = |start: RunStart, rows: Vec<Row>| {
        let run = writer.begin(&start).unwrap_or(RunId::new(0));
        assert_eq!(writer.record(run, rows), Ok(()));
        assert_eq!(writer.finish(run, end(b"")), Ok(()));
    };
    let tested = |calls: Vec<ExchangeRow>| Row::Test(TestRow { exchanges: calls, ..failed("users.crud#a") });
    let (get, create) = (("users#get", "GET"), ("users#create", "POST"));
    stored(start(&["dev"]), vec![tested(vec![call(get, Some(200), 100.0)])]);
    stored(RunStart { kind: "load".into(), ..start(&["dev"]) }, vec![tested(vec![call(get, Some(200), 9e3)])]);
    let unnamed = ExchangeRow { api_id: None, ..call(get, Some(200), 1.0) };
    let rows = vec![
        tested(vec![call(get, Some(500), 30.0), call(get, None, 5e3), unnamed]),
        Row::Exchange(call(create, Some(201), 10.0)),
    ];
    stored(start(&["dev"]), rows);
    stored(start(&["staging"]), vec![tested(vec![call(get, Some(200), 10.0)])]);
    let trend = |(api_id, method): (&str, &str), calls, p50_ms, p95_ms, statuses: &[u16]| ApiTrend {
        api_id: api_id.into(),
        method: method.into(),
        calls,
        p50_ms: Some(p50_ms),
        p95_ms: Some(p95_ms),
        statuses: statuses.to_vec(),
    };
    let created = trend(create, 1, 10.0, 10.0, &[201]);
    let newest = vec![created.clone(), trend(get, 3, 10.0, 30.0, &[200, 500])];
    let message = "the newest 2 `run` runs, lifecycle calls and those with no response counted, unnamed ones left out";
    assert_eq!(reader.api_trends(2, &RunFilter::default()), Ok(newest), "{message}");
    let under_dev = vec![created, trend(get, 3, 30.0, 100.0, &[200, 500])];
    assert_eq!(reader.api_trends(9, &filter(None, Some("dev"), None)), Ok(under_dev));
    assert_eq!(reader.api_trends(9, &filter(None, None, Some("feat/x"))), Ok(Vec::new()));
}
