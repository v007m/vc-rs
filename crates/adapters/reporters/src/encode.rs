//! The JSON shapes the `json` and `ctrf` reporters share.

use serde_json::{Value, json};
use vc_model::{ActionOutcome, CheckResult, CleanupRecord, Failure, Origin, OwnerKey, RunStatus, Totals};

use crate::collect::StepRecord;
use crate::names::{self, Location, Names};

/// `{"kind": "file", "file": "users.crud"}`.
pub fn owner(names: &Names, owner: OwnerKey) -> Value {
    match owner {
        OwnerKey::Run => json!({"kind": "run"}),
        OwnerKey::File(file) => json!({"kind": "file", "file": names.file(file)}),
        OwnerKey::Instance(instance) => json!({"kind": "instance", "instance": instance.index()}),
    }
}

/// A check with the `expect` key it was written under and where.
pub fn check(names: &Names, check: &CheckResult) -> Value {
    json!({
        "key": names::check_label(check),
        "passed": check.passed,
        "expected": check.expected,
        "actual": check.actual,
        "diff": check.diff,
        "error": check.error,
        "code": check.code,
        "origin": origin(names, check.origin),
        "location": location(names.location(check.span)),
    })
}

/// Where an expectation was written: `api users#create`, `test users.crud#getsUser`.
pub fn origin(names: &Names, origin: Origin) -> String {
    match origin {
        Origin::Api(api) => format!("api {}", names.text(api.sym())),
        Origin::Test(test) => format!("test {}", names.test(test)),
        Origin::Action(action) => format!("action {}", names.action(action)),
    }
}

/// `{"path": …, "line": …, "column": …}`, or null.
pub fn location(location: Option<Location>) -> Value {
    location.map_or(Value::Null, |at| json!({"path": at.path, "line": at.line, "column": at.column}))
}

/// `{"message": …, "code": …, "action": …}`.
pub fn failure(names: &Names, failure: &Failure) -> Value {
    json!({
        "message": failure.message,
        "code": failure.code,
        "action": failure.action.map(|action| names.action(action)),
    })
}

/// `"passed"`, or the failure.
pub fn outcome(names: &Names, outcome: &ActionOutcome) -> (&'static str, Value) {
    outcome.failure().map_or(("passed", Value::Null), |failed| ("failed", failure(names, failed)))
}

/// An action that ran, its outcome and checks.
pub fn step(names: &Names, step: &StepRecord) -> Value {
    let (status, failed) = outcome(names, &step.outcome);
    let checks: Vec<Value> = step.checks.iter().map(|c| check(names, c)).collect();
    json!({
        "action": names.action(step.action),
        "owner": owner(names, step.owner),
        "status": status,
        "failure": failed,
        "checks": checks,
    })
}

/// A cleanup that ran.
pub fn cleanup(names: &Names, cleanup: &CleanupRecord) -> Value {
    let (status, failed) = outcome(names, &cleanup.outcome);
    json!({
        "action": names.action(cleanup.action),
        "owner": owner(names, cleanup.owner),
        "status": status,
        "failure": failed,
    })
}

/// Every status's count.
pub fn totals(totals: Totals) -> Value {
    json!({
        "passed": totals.passed,
        "failed": totals.failed,
        "blocked": totals.blocked,
        "flaky": totals.flaky,
        "skipped": totals.skipped,
    })
}

/// `passed`, `failed` or `cancelled`.
pub const fn run_status(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Passed => "passed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
    }
}

#[cfg(test)]
#[path = "encode_tests.rs"]
mod tests;
