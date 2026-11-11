use super::{ActionOutcome, Failure, TestStatus, Totals};
use crate::ActionId;

#[test]
fn failures_carry_code_and_action() {
    let failure = Failure::new("boom").with_code("V5105").at(ActionId::new(2));
    assert_eq!(failure.code.as_deref(), Some("V5105"));
    assert_eq!(failure.action, Some(ActionId::new(2)));
    assert_eq!(failure.message, "boom");
    assert_eq!(ActionOutcome::Failed(failure.clone()).failure(), Some(&failure));
    assert_eq!(ActionOutcome::Passed.failure(), None);
}

#[test]
fn totals_count_each_status_in_its_own_field() {
    let mut totals = Totals::default();
    let statuses = [
        TestStatus::Passed,
        TestStatus::Failed,
        TestStatus::Blocked,
        TestStatus::Flaky,
        TestStatus::Skipped,
        TestStatus::AllowedFailure,
    ];
    for (n, status) in statuses.into_iter().enumerate() {
        for _ in 0..=n {
            totals.add(status);
        }
    }
    assert_eq!(totals, Totals { passed: 1, failed: 2, blocked: 3, flaky: 4, skipped: 5, allowed: 6 });
    assert_eq!((totals.total(), totals.failures()), (21, 5), "an allowed failure does not fail the run (D248)");
}

#[test]
fn merged_totals_add_field_by_field() {
    let mut totals = Totals { passed: 1, failed: 2, blocked: 3, flaky: 4, skipped: 5, allowed: 6 };
    totals.merge(Totals { passed: 10, failed: 20, blocked: 30, flaky: 40, skipped: 50, allowed: 60 });
    assert_eq!(totals, Totals { passed: 11, failed: 22, blocked: 33, flaky: 44, skipped: 55, allowed: 66 });
}
