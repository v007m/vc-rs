//! History rows built from the recorder's views: what `.vibranium/history.db` stores of a test.

use vc_interfaces::{CheckRow, ExchangeRow, TestRow, TimingRow};
use vc_model::{CheckResult, Names};

use crate::collect::TestRecord;
use crate::encode;
use crate::view::{ExchangeView, TestView};

/// A finished test's row, with its checks and its kept exchanges.
pub fn test(names: &Names, record: &TestRecord, view: &TestView) -> TestRow {
    let checks = record.steps.iter().flat_map(|step| &step.checks).map(|c| check(names, c)).collect();
    TestRow {
        test_id: view.id.clone(),
        example_row: view.example_row,
        repeat_index: view.repeat_index,
        status: Some(view.status),
        duration_ms: record.duration().as_secs_f64() * 1_000.0,
        attempts: view.attempts,
        blocked_by: view.blocked_by.clone(),
        skip_reason: view.skip_reason.clone(),
        message: view.message.clone(),
        checks,
        exchanges: view.exchanges.iter().map(exchange).collect(),
    }
}

/// An exchange as the history stores it; the view's bodies are already cut.
pub fn exchange(view: &ExchangeView) -> ExchangeRow {
    let timing = view.timing;
    ExchangeRow {
        action: view.action.clone(),
        api_id: view.api.clone(),
        method: view.method.clone(),
        url: view.url.clone(),
        status: Some(view.status),
        timing: TimingRow {
            dns_ms: timing.dns,
            connect_ms: timing.connect,
            tls_ms: timing.tls,
            first_byte_ms: timing.first_byte,
            download_ms: timing.download,
            total_ms: timing.total,
        },
        request_headers: view.request_headers.clone(),
        response_headers: view.response_headers.clone(),
        request_body: view.request_body.clone().map(String::into_bytes),
        response_body: view.response_body.clone().map(String::into_bytes),
        body_truncated: view.truncated,
    }
}

fn check(names: &Names, check: &CheckResult) -> CheckRow {
    let (kind, path) = vc_model::check_parts(check);
    let location = names.location(check.span);
    CheckRow {
        kind: kind.to_owned(),
        path,
        expected: check.expected.to_string(),
        actual: check.actual.as_ref().map(ToString::to_string),
        passed: check.passed,
        origin: encode::origin(names, check.origin),
        file: location.as_ref().map(|at| at.path.clone()).unwrap_or_default(),
        line: location.map_or(0, |at| at.line),
    }
}
