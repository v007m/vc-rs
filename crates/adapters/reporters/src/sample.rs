//! A synthetic run of any size, for the report budgets: the `report/html_10k` bench and the
//! data-size test. Not part of the reporting API.

use serde_json::Value;
use vc_model::{RunStatus, TestStatus, Totals};

use crate::view::{CheckView, ExchangeView, FailureView, Gathered, OwnerView, StepView, TestView, TimingView};
use crate::{RunFacts, RunSnapshot};

/// Tests per file in the synthetic run.
const PER_FILE: usize = 50;
/// One test in this many fails, with an exchange and a failed check.
const FAILING: usize = 10;

/// A finished run of `tests` tests in files of 50, one in ten failed with its exchange.
#[doc(hidden)]
#[must_use]
pub fn sample_snapshot(tests: usize) -> RunSnapshot {
    let views: Vec<TestView> = (0..tests).map(test).collect();
    let mut totals = Totals::default();
    views.iter().for_each(|view| totals.add(view.status));
    let files = (0..tests.div_ceil(PER_FILE)).map(file).collect();
    let run = RunFacts {
        id: 1,
        status: if tests >= FAILING { RunStatus::Failed } else { RunStatus::Passed },
        totals,
        started_ms: 1_793_750_400_000,
        duration_ms: 60_000,
        profiles: vec!["dev".to_owned()],
        seed: Some(42),
    };
    RunSnapshot::new(run, Gathered { files, tests: views, ..Gathered::default() })
}

fn file(index: usize) -> String {
    format!("service{index}.area")
}

fn test(index: usize) -> TestView {
    let file = file(index / PER_FILE);
    let failed = index % FAILING == FAILING - 1;
    let id = format!("{file}#checksThing{index}");
    let start = 1_793_750_400_000 + u64::try_from(index).unwrap_or(0);
    TestView {
        label: format!("Checks thing {index}"),
        status: if failed { TestStatus::Failed } else { TestStatus::Passed },
        instance: u32::try_from(index).ok(),
        example_row: None,
        repeat_index: 0,
        attempts: 1,
        start_ms: Some(start),
        stop_ms: start + 12,
        location: None,
        tags: vec!["smoke".to_owned()],
        failure: failed.then(|| FailureView { message: "expected 200, got 500".to_owned(), code: None, action: None }),
        blocked_by: None,
        skip_reason: None,
        message: failed.then(|| "expected 200, got 500".to_owned()),
        detail: String::new(),
        steps: vec![step(&id, failed)],
        exchanges: if failed { vec![exchange(index)] } else { Vec::new() },
        id,
        file,
    }
}

fn step(id: &str, failed: bool) -> StepView {
    let check = CheckView {
        key: "status".to_owned(),
        passed: !failed,
        expected: Value::from(200),
        actual: Some(Value::from(if failed { 500 } else { 200 })),
        diff: None,
        error: None,
        code: None,
        origin: format!("test {id}"),
        location: None,
    };
    let status = if failed { "failed" } else { "passed" };
    StepView { action: "call".to_owned(), owner: OwnerView::Run, status: status.to_owned(), failure: None, checks: vec![check] }
}

fn exchange(index: usize) -> ExchangeView {
    ExchangeView {
        action: "call".to_owned(),
        api: Some("users#get".to_owned()),
        method: "GET".to_owned(),
        url: format!("http://localhost/users/{index}"),
        status: 500,
        timing: TimingView { first_byte: 10.0, download: 1.0, total: 12.0, ..TimingView::default() },
        request_headers: vec![("accept".to_owned(), "application/json".to_owned())],
        response_headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        request_body: None,
        response_body: Some(format!("{{\"error\":\"boom\",\"user\":{index}}}")),
        truncated: false,
    }
}
