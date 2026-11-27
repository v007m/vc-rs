//! A load run's report on the wire (D678), as `vc load --json` gives it.

use vc_contracts::engine::v1 as wire;
use vc_engine::{ErrorKind, LoadBreach, LoadEnding, LoadReport, LoadTestReport};

/// `report` on the wire.
pub fn load_report(report: LoadReport) -> wire::LoadReport {
    let LoadReport { exit, scenario, name, ending, seconds, iterations, errors, rps, .. } = report;
    let (p50_ms, p90_ms, p95_ms, p99_ms, max_ms) =
        (report.p50_ms, report.p90_ms, report.p95_ms, report.p99_ms, report.max_ms);
    wire::LoadReport {
        exit: u32::from(exit),
        scenario,
        name,
        ending: ended(ending).into(),
        seconds,
        iterations,
        errors,
        rps,
        p50_ms,
        p90_ms,
        p95_ms,
        p99_ms,
        max_ms,
        breaches: report.breaches.into_iter().map(breach).collect(),
        tests: report.tests.into_iter().map(test).collect(),
        error_kinds: report.error_kinds.into_iter().map(error_kind).collect(),
        blocked: report.blocked,
        run: report.run_id,
        seed: report.seed,
        notices: report.notices,
    }
}

/// How the run ended on the wire; an ending this server does not know is unspecified.
const fn ended(ending: LoadEnding) -> wire::LoadEnding {
    match ending {
        LoadEnding::Finished => wire::LoadEnding::Finished,
        LoadEnding::Aborted => wire::LoadEnding::Aborted,
        LoadEnding::Cancelled => wire::LoadEnding::Cancelled,
        LoadEnding::Blocked => wire::LoadEnding::Blocked,
        _ => wire::LoadEnding::Unspecified,
    }
}

fn breach(breach: LoadBreach) -> wire::LoadBreach {
    let LoadBreach { metric, test, actual, unit, limit, shown, .. } = breach;
    wire::LoadBreach { metric, test, actual, unit, limit_json: limit.to_string(), shown }
}

fn test(test: LoadTestReport) -> wire::LoadTestReport {
    let LoadTestReport { test, iterations, errors, rps, p50_ms, p90_ms, p95_ms, p99_ms, max_ms, .. } = test;
    wire::LoadTestReport { test, iterations, errors, rps, p50_ms, p90_ms, p95_ms, p99_ms, max_ms }
}

fn error_kind(kind: ErrorKind) -> wire::LoadErrorKind {
    let ErrorKind { api, test, status, code, count, .. } = kind;
    wire::LoadErrorKind { api, test, status: status.map(u32::from), code, count }
}

#[cfg(test)]
#[path = "wire_load_tests.rs"]
mod tests;
