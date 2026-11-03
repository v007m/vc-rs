use vc_model::{RunStatus, Totals};

use super::{EXIT_CANCELLED, EXIT_FAILED, EXIT_NOTHING_RAN, EXIT_PASSED, exit_code};
use crate::RunSummary;

fn summary(status: RunStatus, totals: Totals) -> RunSummary {
    RunSummary { status, totals }
}

#[test]
fn exit_codes_follow_d127() {
    let one = |field: fn(&mut Totals)| {
        let mut totals = Totals::default();
        field(&mut totals);
        totals
    };
    let cases = [
        (summary(RunStatus::Passed, one(|t| t.passed = 1)), EXIT_PASSED),
        (summary(RunStatus::Passed, one(|t| t.flaky = 1)), EXIT_PASSED),
        (summary(RunStatus::Passed, one(|t| t.skipped = 1)), EXIT_PASSED),
        (summary(RunStatus::Failed, one(|t| t.failed = 1)), EXIT_FAILED),
        (summary(RunStatus::Failed, one(|t| t.blocked = 1)), EXIT_FAILED),
        (summary(RunStatus::Passed, Totals::default()), EXIT_NOTHING_RAN),
        (summary(RunStatus::Failed, Totals::default()), EXIT_NOTHING_RAN),
        (summary(RunStatus::Cancelled, Totals::default()), EXIT_CANCELLED),
        (summary(RunStatus::Cancelled, one(|t| t.failed = 1)), EXIT_CANCELLED),
    ];
    for (summary, code) in cases {
        assert_eq!(exit_code(summary), code, "{summary:?}");
    }
}
