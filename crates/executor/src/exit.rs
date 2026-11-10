//! The exit code of a functional run (D127).

use vc_model::RunStatus;

use crate::RunSummary;

/// Every test passed, was skipped or was flaky.
pub const EXIT_PASSED: u8 = 0;
/// A test failed or was blocked.
pub const EXIT_FAILED: u8 = 1;
/// The selection was empty: nothing ran.
pub const EXIT_NOTHING_RAN: u8 = 2;
/// The run was cancelled (Ctrl-C).
pub const EXIT_CANCELLED: u8 = 130;

/// The exit code for a finished run (D127). Load thresholds (3), drift (4) and internal errors
/// (70) are decided outside the executor.
#[must_use]
pub const fn exit_code(summary: RunSummary) -> u8 {
    match summary.status {
        RunStatus::Cancelled => EXIT_CANCELLED,
        _ if summary.totals.total() == 0 => EXIT_NOTHING_RAN,
        RunStatus::Failed => EXIT_FAILED,
        RunStatus::Passed => EXIT_PASSED,
    }
}

#[cfg(test)]
#[path = "exit_tests.rs"]
mod tests;
