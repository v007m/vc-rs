//! The shapes the `json` reporter and the run snapshot share, built from events and names.

use serde_json::{Value, json};
use vc_model::{
    ActionOutcome, CheckResult, CleanupRecord, Event, Exchange, Failure, Names, Origin, OwnerKey, RunStatus, Totals,
};

use crate::collect::StepRecord;
use crate::view::{CheckView, CleanupView, FailureView, OwnerView, StepView};

/// `{"kind": "file", "file": "users.crud"}`.
pub fn owner(names: &Names, owner: OwnerKey) -> OwnerView {
    match owner {
        OwnerKey::Run => OwnerView::Run,
        OwnerKey::File(file) => OwnerView::File { file: names.file(file).to_owned() },
        OwnerKey::Instance(instance) => OwnerView::Instance { instance: instance.index() },
    }
}

/// A check with the `expect` key it was written under and where.
pub fn check(names: &Names, check: &CheckResult) -> CheckView {
    CheckView {
        key: vc_model::check_label(check),
        passed: check.passed,
        expected: check.expected.clone(),
        actual: check.actual.clone(),
        diff: check.diff.clone(),
        error: check.error.clone(),
        code: check.code.clone(),
        origin: origin(names, check.origin),
        location: names.location(check.span),
    }
}

/// Where an expectation was written: `api users#create`, `test users.crud#getsUser`.
pub fn origin(names: &Names, origin: Origin) -> String {
    match origin {
        Origin::Api(api) => format!("api {}", names.text(api.sym())),
        Origin::Test(test) => format!("test {}", names.test(test)),
        Origin::Action(action) => format!("action {}", names.action(action)),
    }
}

/// `{"message": …, "code": …, "action": …}`.
pub fn failure(names: &Names, failure: &Failure) -> FailureView {
    FailureView {
        message: failure.message.clone(),
        code: failure.code.clone(),
        action: failure.action.map(|action| names.action(action)),
    }
}

/// `"passed"`, or the failure.
pub fn outcome(names: &Names, outcome: &ActionOutcome) -> (&'static str, Option<FailureView>) {
    outcome.failure().map_or(("passed", None), |failed| ("failed", Some(failure(names, failed))))
}

/// An action that ran, its outcome and checks.
pub fn step(names: &Names, step: &StepRecord) -> StepView {
    let (status, failed) = outcome(names, &step.outcome);
    StepView {
        action: names.action(step.action),
        owner: owner(names, step.owner),
        status: status.to_owned(),
        failure: failed,
        checks: step.checks.iter().map(|c| check(names, c)).collect(),
    }
}

/// A cleanup that ran.
pub fn cleanup(names: &Names, cleanup: &CleanupRecord) -> CleanupView {
    let (status, failed) = outcome(names, &cleanup.outcome);
    CleanupView {
        action: names.action(cleanup.action),
        owner: owner(names, cleanup.owner),
        status: status.to_owned(),
        failure: failed,
    }
}

/// An exchange's method, URL, status and total time; headers and bodies stay in the history.
pub fn exchange(exchange: &Exchange) -> Value {
    json!({
        "method": exchange.request.method.as_str(),
        "url": exchange.request.uri.to_string(),
        "status": exchange.response.status.as_u16(),
        "ms": u64::try_from(exchange.timing.total.as_millis()).unwrap_or(u64::MAX),
    })
}

/// Counts by status.
pub fn totals(totals: Totals) -> Value {
    json!({
        "passed": totals.passed,
        "failed": totals.failed,
        "blocked": totals.blocked,
        "flaky": totals.flaky,
        "skipped": totals.skipped,
        "allowedFailures": totals.allowed_failures,
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

/// A load run's second as the live view shows it; any other event is empty.
pub fn tick(event: &Event) -> Value {
    let Event::LoadTick { second, users, rps, p50, p90, p95, p99, errors, breaching } = event else { return json!({}) };
    let ms = |latency: &std::time::Duration| latency.as_secs_f64() * 1000.0;
    json!({
        "second": second,
        "users": users,
        "rps": rps,
        "p50Ms": ms(p50),
        "p90Ms": ms(p90),
        "p95Ms": ms(p95),
        "p99Ms": ms(p99),
        "errors": errors,
        "breaching": breaching,
    })
}

#[cfg(test)]
#[path = "encode_tests.rs"]
mod tests;
