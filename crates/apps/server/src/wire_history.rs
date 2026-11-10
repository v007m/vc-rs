//! Stored runs in their wire form, as `vc history --json` reads them. A stored check keeps its
//! file and line only, and a stored exchange no wire size, reuse or protocol, so those are left
//! at their defaults.

use vc_contracts::results::v1 as wire;
use vc_engine::{CheckRow, ExchangeRow, RunRow, RunStatus, TestRow, TestStatus, Totals};

/// Each stored check kind by the key history keeps it under.
const CHECK_KINDS: [(&str, wire::CheckKind); 9] = [
    ("status", wire::CheckKind::Status),
    ("headers", wire::CheckKind::Header),
    ("body.schema", wire::CheckKind::Schema),
    ("body.match", wire::CheckKind::Match),
    ("body.equals", wire::CheckKind::Equals),
    ("size", wire::CheckKind::Size),
    ("timing", wire::CheckKind::Timing),
    ("cel", wire::CheckKind::Cel),
    ("custom", wire::CheckKind::Custom),
];

/// A stored run.
pub fn run(row: &RunRow) -> wire::Run {
    let start = &row.start;
    wire::Run {
        id: row.id.get(),
        kind: start.kind.clone(),
        started_ms: start.started_ms,
        finished_ms: row.finished_ms,
        profiles: start.profiles.clone(),
        selector: start.selector.clone(),
        seed: start.seed,
        git_commit: start.git_commit.clone(),
        git_branch: start.git_branch.clone(),
        vc_version: start.vc_version.clone(),
        status: row.status.map_or(wire::RunStatus::Unspecified, run_status).into(),
        totals: Some(totals(&row.totals)),
    }
}

/// A stored test result, labelled as the CLI labels it: `users.crud#rejectsBadEmail [row 2]`.
pub fn test(row: &TestRow) -> wire::Test {
    wire::Test {
        id: row.test_id.clone(),
        label: label(row),
        example_row: row.example_row,
        repeat_index: row.repeat_index,
        status: row.status.map_or(wire::TestStatus::Unspecified, test_status).into(),
        attempts: row.attempts,
        duration_ms: row.duration_ms,
        failure: row.message.clone().map(|message| wire::Failure { message, ..wire::Failure::default() }),
        blocked_by: row.blocked_by.clone(),
        skip_reason: row.skip_reason.clone(),
        checks: row.checks.iter().map(check).collect(),
        exchanges: row.exchanges.iter().map(exchange).collect(),
    }
}

fn label(row: &TestRow) -> String {
    let row_mark = row.example_row.map(|index| format!("row {}", u64::from(index) + 1));
    let repeat = (row.repeat_index > 0).then(|| format!("repeat {}", u64::from(row.repeat_index) + 1));
    let marks: Vec<String> = row_mark.into_iter().chain(repeat).collect();
    if marks.is_empty() { row.test_id.clone() } else { format!("{} [{}]", row.test_id, marks.join(", ")) }
}

fn check(row: &CheckRow) -> wire::Check {
    let kind =
        CHECK_KINDS.iter().find(|(key, _)| *key == row.kind).map_or(wire::CheckKind::Unspecified, |(_, kind)| *kind);
    let span =
        wire::Span { file: row.file.clone(), start_line: row.line, start_column: 1, end_line: row.line, end_column: 1 };
    wire::Check {
        kind: kind.into(),
        target: row.path.clone(),
        origin: row.origin.clone(),
        span: Some(span),
        passed: row.passed,
        expected: row.expected.clone(),
        actual: row.actual.clone(),
        ..wire::Check::default()
    }
}

fn exchange(row: &ExchangeRow) -> wire::Exchange {
    let timing = &row.timing;
    wire::Exchange {
        action: row.action.clone(),
        api: row.api_id.clone(),
        method: row.method.clone(),
        url: row.url.clone(),
        status: row.status.map(u32::from),
        timing: Some(wire::Timing {
            dns_ms: timing.dns_ms,
            connect_ms: timing.connect_ms,
            tls_ms: timing.tls_ms,
            first_byte_ms: timing.first_byte_ms,
            download_ms: timing.download_ms,
            total_ms: timing.total_ms,
        }),
        request_headers: headers(&row.request_headers),
        response_headers: headers(&row.response_headers),
        request_body: row.request_body.clone().unwrap_or_default(),
        response_body: row.response_body.clone().unwrap_or_default(),
        body_truncated: row.body_truncated,
        ..wire::Exchange::default()
    }
}

fn headers(pairs: &[(String, String)]) -> Vec<wire::Header> {
    pairs.iter().map(|(name, value)| wire::Header { name: name.clone(), value: value.clone() }).collect()
}

/// A run's status.
pub const fn run_status(status: RunStatus) -> wire::RunStatus {
    match status {
        RunStatus::Passed => wire::RunStatus::Passed,
        RunStatus::Failed => wire::RunStatus::Failed,
        RunStatus::Cancelled => wire::RunStatus::Cancelled,
    }
}

/// A test's status.
pub const fn test_status(status: TestStatus) -> wire::TestStatus {
    match status {
        TestStatus::Passed => wire::TestStatus::Passed,
        TestStatus::Failed => wire::TestStatus::Failed,
        TestStatus::Blocked => wire::TestStatus::Blocked,
        TestStatus::Flaky => wire::TestStatus::Flaky,
        TestStatus::Skipped => wire::TestStatus::Skipped,
    }
}

/// A run's totals; a count past `u32::MAX` saturates.
pub fn totals(totals: &Totals) -> wire::Totals {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    wire::Totals {
        passed: count(totals.passed),
        failed: count(totals.failed),
        blocked: count(totals.blocked),
        flaky: count(totals.flaky),
        skipped: count(totals.skipped),
    }
}

#[cfg(test)]
#[path = "wire_history_tests.rs"]
mod tests;
