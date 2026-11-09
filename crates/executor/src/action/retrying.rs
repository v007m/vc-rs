//! `retry` on an action: after a failure the action runs again, up to `attempts` more times with
//! `delay` between, and the failed attempt's cleanups run first, newest first (D126). The last
//! attempt's report stands.

use vc_model::{ActionOutcome, Retry};

use super::{ActionExecutors, ActionRequest};
use crate::{ActionReport, PendingCleanup};

/// Runs `request` with `executors`, again after each failure while `retry` allows.
pub async fn attempt(executors: &ActionExecutors, request: ActionRequest, retry: Retry) -> ActionReport {
    let mut report = executors.once(request.clone()).await;
    for _ in 0..retry.attempts {
        if !matches!(report.outcome, ActionOutcome::Failed(_)) {
            break;
        }
        undo(executors, &request, &report.cleanups).await;
        tokio::time::sleep(retry.delay.get()).await;
        report = executors.once(request.clone()).await;
    }
    report
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
