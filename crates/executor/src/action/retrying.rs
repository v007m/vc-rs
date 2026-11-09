//! `retry` on an action: after a failure the action runs again, up to `attempts` more times with
//! `delay` between, and the failed attempt's cleanups run first, newest first (D126). The last
//! attempt's report stands.

use vc_model::{ActionOutcome, Retry};

use super::{ActionExecutors, ActionRequest};
use crate::{ActionReport, CleanupCollector, PendingCleanup};

/// Runs `request` with `executors`, again after each failure while `retry` allows.
pub async fn attempt(executors: &ActionExecutors, request: ActionRequest, retry: Retry) -> ActionReport {
    let mut report = one_attempt(executors, &request).await;
    for _ in 0..retry.attempts {
        if !matches!(report.outcome, ActionOutcome::Failed(_)) {
            break;
        }
        undo(executors, &request, &report.cleanups).await;
        tokio::time::sleep(retry.delay.get()).await;
        report = one_attempt(executors, &request).await;
    }
    report
}

/// One attempt, with every cleanup it registered in its report, those its parts handed over
/// included, so a failed attempt undoes them all; dropped, it hands them to the frame's sink.
async fn one_attempt(executors: &ActionExecutors, request: &ActionRequest) -> ActionReport {
    CleanupCollector::around(request.clone(), |request| executors.once(request)).await
}

/// Runs a failed attempt's `cleanups` newest first in `request`'s frame; how they end changes
/// nothing, as for any cleanup.
async fn undo(executors: &ActionExecutors, request: &ActionRequest, cleanups: &[PendingCleanup]) {
    for cleanup in cleanups.iter().rev() {
        let frame = request.frame.cleaning(&cleanup.vars);
        let _ended = executors.once(ActionRequest { action: cleanup.action, frame, ..request.clone() }).await;
    }
}

#[cfg(test)]
#[path = "retrying_tests.rs"]
mod tests;
