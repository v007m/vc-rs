//! What a load run found, as the history keeps it (D322): each breached threshold and each
//! kind of error, with the line the run printed for it.

use std::fmt::Write as _;

use serde_json::{Value, json};
use vc_interfaces::{FindingKind, FindingRow, Row};
use vc_model::one_line;

use crate::{ErrorKind, LoadBreach, LoadReport};

/// `report`'s breaches, then its error kinds, as history rows.
pub fn rows(report: &LoadReport) -> Vec<Row> {
    let breaches = report.breaches.iter().map(breach);
    breaches.chain(report.error_kinds.iter().map(error)).map(Row::Finding).collect()
}

fn breach(breach: &LoadBreach) -> FindingRow {
    FindingRow {
        kind: FindingKind::Breach,
        test_id: breach.test.clone(),
        metric: Some(breach.metric.clone()),
        actual: Some(breach.actual),
        shown: breach.line(),
        ..FindingRow::default()
    }
}

fn error(kind: &ErrorKind) -> FindingRow {
    FindingRow {
        kind: FindingKind::Error,
        test_id: kind.test.clone(),
        api_id: kind.api.clone(),
        status: kind.status,
        code: kind.code.clone(),
        count: Some(kind.count),
        shown: kind.summary(),
        ..FindingRow::default()
    }
}

/// The lines `vc history show` prints under a load run's line: its breaches, then each test
/// with its errors under it, every stored text on one line with what would drive a terminal
/// replaced.
pub fn text(findings: &[FindingRow]) -> String {
    let mut out = String::new();
    let (breaches, errors): (Vec<_>, Vec<_>) = findings.iter().partition(|found| found.kind == FindingKind::Breach);
    for breach in breaches {
        let _ = writeln!(out, "  {}", one_line(&breach.shown));
    }
    let mut tests: Vec<Option<&str>> = Vec::new();
    for error in &errors {
        if !tests.contains(&error.test_id.as_deref()) {
            tests.push(error.test_id.as_deref());
        }
    }
    for test in tests {
        let _ = writeln!(out, "  test {}", one_line(test.unwrap_or_default()));
        for error in errors.iter().filter(|error| error.test_id.as_deref() == test) {
            let _ = writeln!(out, "    {}", one_line(&error.shown));
        }
    }
    out
}

/// A stored finding as JSON, its keys those of the load report's `breaches` and `errorKinds`.
pub fn json(found: &FindingRow) -> Value {
    json!({
        "kind": found.kind.as_str(),
        "test": found.test_id,
        "api": found.api_id,
        "metric": found.metric,
        "status": found.status,
        "code": found.code,
        "count": found.count,
        "actual": found.actual,
        "shown": found.shown,
    })
}

#[cfg(test)]
#[path = "load_findings_tests.rs"]
mod tests;
