//! Stored runs in their wire form, as `vc history --json` reads them. A stored check keeps its
//! file and line only, and a stored exchange no wire size, reuse or protocol, so those are left
//! at their defaults.

use vc_contracts::results::v1 as wire;
use vc_engine::{CheckKind, CheckRow, ExchangeRow, RunRow, TestRow, instance_label};

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
        status: row.status.map_or(wire::RunStatus::Unspecified, wire::RunStatus::from).into(),
        totals: Some(row.totals.into()),
    }
}

/// A stored test result, labelled as the run labelled it: `users.crud#rejectsBadEmail › missing @`.
pub fn test(row: &TestRow) -> wire::Test {
    wire::Test {
        id: row.test_id.clone(),
        label: instance_label(row),
        example_row: row.example_row,
        repeat_index: row.repeat_index,
        status: row.status.map_or(wire::TestStatus::Unspecified, wire::TestStatus::from).into(),
        attempts: row.attempts,
        duration_ms: row.duration_ms,
        failure: row.message.clone().map(|message| wire::Failure { message, ..wire::Failure::default() }),
        blocked_by: row.blocked_by.clone(),
        skip_reason: row.skip_reason.clone(),
        checks: row.checks.iter().map(check).collect(),
        exchanges: row.exchanges.iter().map(exchange).collect(),
    }
}

fn check(row: &CheckRow) -> wire::Check {
    let kind = CheckKind::from_key(&row.kind).map_or(wire::CheckKind::Unspecified, wire::CheckKind::from);
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

#[cfg(test)]
#[path = "wire_history_tests.rs"]
mod tests;
