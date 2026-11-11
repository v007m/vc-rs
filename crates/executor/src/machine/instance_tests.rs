use std::sync::Arc;
use std::time::Duration;

use vc_model::{AllowFailure, Failure, Retry, TestStatus};

use super::super::sim;
use super::super::{ActionReport, PendingCleanup};
use super::{InstanceCommand, InstanceInput, InstancePhase, InstanceScript, InstanceWorkflow};

/// before = [1, 2], body = [10, 11, 12], after = [20, 21].
fn script(retry: Option<Retry>) -> Arc<InstanceScript> {
    Arc::new(InstanceScript {
        before: sim::acts(&[1, 2]),
        body: sim::acts(&[10, 11, 12]),
        after: sim::acts(&[20, 21]),
        retry,
        allow_failure: None,
    })
}

fn retry(attempts: u32) -> Retry {
    Retry { attempts, delay: vc_model::Duration::new(Duration::from_millis(250)) }
}

/// Fails `fails`, then expects these actions run, this status, and this action to blame.
type Case = (&'static [u32], &'static [u32], TestStatus, u32);

/// Fails the actions in `failing`; passes the rest.
fn failing(failing: &[u32]) -> impl FnMut(vc_model::ActionId) -> ActionReport {
    let failing = failing.to_vec();
    move |action| if failing.contains(&action.index()) { sim::fail(action.index()) } else { ActionReport::passed() }
}

fn run(retry: Option<Retry>, fails: &[u32]) -> Vec<InstanceCommand> {
    sim::drive_instance(InstanceWorkflow::new(script(retry)), InstanceInput::Start, &mut failing(fails))
}

#[test]
fn a_clean_instance_runs_before_body_and_after_in_order() {
    let log = run(None, &[]);
    assert_eq!(sim::ran(&log), [1, 2, 10, 11, 12, 20, 21]);
    let verdict = sim::verdict(&log);
    assert_eq!((verdict.status, verdict.attempts, verdict.failure), (TestStatus::Passed, 1, None));
}

#[test]
fn table_of_single_failures() {
    let cases: &[Case] = &[
        (&[1], &[1, 20, 21], TestStatus::Blocked, 1),
        (&[2], &[1, 2, 20, 21], TestStatus::Blocked, 2),
        (&[10], &[1, 2, 10, 20, 21], TestStatus::Failed, 10),
        (&[11], &[1, 2, 10, 11, 20, 21], TestStatus::Failed, 11),
        (&[20], &[1, 2, 10, 11, 12, 20, 21], TestStatus::Failed, 20),
        (&[11, 20], &[1, 2, 10, 11, 20, 21], TestStatus::Failed, 11),
        (&[20, 21], &[1, 2, 10, 11, 12, 20, 21], TestStatus::Failed, 20),
        (&[1, 10], &[1, 20, 21], TestStatus::Blocked, 1),
    ];
    for &(fails, expected_ran, status, culprit) in cases {
        let log = run(None, fails);
        assert_eq!(sim::ran(&log), expected_ran, "fails {fails:?}");
        let verdict = sim::verdict(&log);
        assert_eq!(verdict.status, status, "fails {fails:?}");
        assert_eq!(
            verdict.failure.and_then(|f| f.action).map(vc_model::ActionId::index),
            Some(culprit),
            "fails {fails:?}"
        );
    }
}

#[test]
fn cleanups_run_newest_first_after_after_on_pass_and_fail() {
    for fails in [&[][..], &[11], &[20]] {
        let mut respond = |action: vc_model::ActionId| match action.index() {
            10 => sim::passing_with(&[100, 101]),
            n if fails.contains(&n) => sim::fail(n),
            11 => sim::passing_with(&[102]),
            _ => ActionReport::passed(),
        };
        let log = sim::drive_instance(InstanceWorkflow::new(script(None)), InstanceInput::Start, &mut respond);
        let expected: &[u32] = if fails.contains(&11) { &[101, 100] } else { &[102, 101, 100] };
        assert_eq!(sim::cleaned(&log), expected, "fails {fails:?}");
        let last_action = log.iter().rposition(|c| matches!(c, InstanceCommand::RunAction(_)));
        let first_cleanup = log.iter().position(|c| matches!(c, InstanceCommand::RunCleanup(_)));
        assert!(last_action < first_cleanup, "cleanups come after every action");
    }
}

#[test]
fn a_pass_after_a_failed_attempt_is_flaky() {
    let mut attempt = 0;
    let mut respond = |action: vc_model::ActionId| {
        if action.index() == 1 {
            attempt += 1;
        }
        if action.index() == 10 && attempt == 1 { sim::fail(10) } else { ActionReport::passed() }
    };
    let log = sim::drive_instance(InstanceWorkflow::new(script(Some(retry(2)))), InstanceInput::Start, &mut respond);
    assert!(log.contains(&InstanceCommand::Wait(Duration::from_millis(250))));
    let verdict = sim::verdict(&log);
    assert_eq!((verdict.status, verdict.attempts), (TestStatus::Flaky, 2));
    assert_eq!(verdict.failure.and_then(|f| f.action).map(vc_model::ActionId::index), Some(10));
}

#[test]
fn retries_run_cleanups_between_attempts_and_stop_when_exhausted() {
    let mut respond = |action: vc_model::ActionId| match action.index() {
        10 => sim::passing_with(&[100]),
        11 => sim::fail(11),
        _ => ActionReport::passed(),
    };
    let log = sim::drive_instance(InstanceWorkflow::new(script(Some(retry(2)))), InstanceInput::Start, &mut respond);
    assert_eq!(sim::cleaned(&log), [100, 100, 100]);
    let waits = log.iter().filter(|c| matches!(c, InstanceCommand::Wait(_))).count();
    let verdict = sim::verdict(&log);
    assert_eq!((waits, verdict.status, verdict.attempts), (2, TestStatus::Failed, 3));
    let wait = log.iter().position(|c| matches!(c, InstanceCommand::Wait(_)));
    assert!(log.iter().position(|c| matches!(c, InstanceCommand::RunCleanup(_))) < wait);
}

/// Runs `retry` and `fails` under `allowFailure`, with `reason` (D248).
fn run_allowed(retry: Option<Retry>, fails: &[u32], reason: Option<&str>) -> Vec<InstanceCommand> {
    let allow = AllowFailure { reason: reason.map(str::to_owned), until: None };
    let script = Arc::new(InstanceScript { allow_failure: Some(allow), ..(*script(retry)).clone() });
    sim::drive_instance(InstanceWorkflow::new(script), InstanceInput::Start, &mut failing(fails))
}

#[test]
fn a_failure_under_allow_failure_is_allowed_after_its_retries_with_its_reason() {
    let log = run_allowed(Some(retry(1)), &[11], Some("see 212"));
    let verdict = sim::verdict(&log);
    assert_eq!((verdict.status, verdict.attempts), (TestStatus::AllowedFailure, 2), "retried first");
    assert_eq!(verdict.skip_reason.as_deref(), Some("see 212"));
    assert_eq!(verdict.failure.and_then(|f| f.action).map(vc_model::ActionId::index), Some(11), "the failure is kept");
    let plain = sim::verdict(&run_allowed(None, &[11], None));
    assert_eq!((plain.status, plain.skip_reason), (TestStatus::AllowedFailure, None));
}

#[test]
fn allow_failure_leaves_a_pass_and_a_block_as_they_are() {
    assert_eq!(sim::verdict(&run_allowed(None, &[], Some("x"))).status, TestStatus::Passed);
    let blocked = sim::verdict(&run_allowed(None, &[1], Some("x")));
    assert_eq!((blocked.status, blocked.skip_reason), (TestStatus::Blocked, None), "a failed setup still blocks");
}

#[test]
fn blocked_instances_are_never_retried() {
    let log = run(Some(retry(3)), &[1]);
    let verdict = sim::verdict(&log);
    assert_eq!((verdict.status, verdict.attempts), (TestStatus::Blocked, 1));
    assert!(!log.iter().any(|c| matches!(c, InstanceCommand::Wait(_))));
}

#[test]
fn a_block_at_start_runs_nothing() {
    let failure = Failure::new("setup failed");
    let log = sim::drive_instance(
        InstanceWorkflow::new(script(Some(retry(3)))),
        InstanceInput::Block(failure.clone()),
        &mut failing(&[]),
    );
    assert_eq!(sim::ran(&log).len(), 0);
    let verdict = sim::verdict(&log);
    assert_eq!((verdict.status, verdict.attempts, verdict.failure), (TestStatus::Blocked, 0, Some(failure)));
}

#[test]
fn cancelled_before_start_is_skipped() {
    for input in [InstanceInput::Cancelled, InstanceInput::Abort] {
        let log = sim::drive_instance(InstanceWorkflow::new(script(None)), input, &mut failing(&[]));
        let verdict = sim::verdict(&log);
        assert_eq!((verdict.status, verdict.attempts), (TestStatus::Skipped, 0));
        assert_eq!(verdict.skip_reason.as_deref(), Some("cancelled"));
    }
}

/// Starts the instance and feeds the first body action's report, which registers cleanup 100.
fn in_body() -> InstanceWorkflow {
    let (workflow, _) = InstanceWorkflow::new(script(Some(retry(2)))).transition(InstanceInput::Start);
    let (workflow, _) = workflow.transition(InstanceInput::ActionDone(ActionReport::passed()));
    let (workflow, _) = workflow.transition(InstanceInput::ActionDone(ActionReport::passed()));
    let (workflow, commands) = workflow.transition(InstanceInput::ActionDone(sim::passing_with(&[100])));
    assert_eq!(commands, [InstanceCommand::RunAction(sim::acts(&[11])[0])]);
    workflow
}

#[test]
fn cancel_in_flight_lets_the_action_finish_then_runs_cleanups_and_skips() {
    let (workflow, commands) = in_body().transition(InstanceInput::Cancelled);
    assert_eq!(commands.len(), 0);
    let (workflow, commands) = workflow.transition(InstanceInput::ActionDone(sim::passing_with(&[101])));
    assert_eq!(commands, [InstanceCommand::RunCleanup(PendingCleanup::new(sim::acts(&[101])[0]))]);
    let (workflow, _) = workflow.transition(InstanceInput::CleanupDone);
    let (workflow, commands) = workflow.transition(InstanceInput::CleanupDone);
    assert_eq!(workflow.phase(), InstancePhase::Done(TestStatus::Skipped));
    assert!(matches!(&commands[..], [InstanceCommand::Finish(v)] if v.skip_reason.as_deref() == Some("cancelled")));
}

#[test]
fn a_failure_reported_after_cancel_is_kept_and_not_retried() {
    let (workflow, _) = in_body().transition(InstanceInput::Cancelled);
    let (workflow, _) = workflow.transition(InstanceInput::ActionDone(sim::fail(11)));
    let (workflow, commands) = workflow.transition(InstanceInput::CleanupDone);
    assert_eq!(workflow.phase(), InstancePhase::Done(TestStatus::Failed));
    assert!(matches!(&commands[..], [InstanceCommand::Finish(v)] if v.attempts == 1));
}

#[test]
fn a_cut_action_runs_cleanups_and_skips() {
    let kept = PendingCleanup::new(sim::acts(&[101])[0]);
    let (workflow, commands) = in_body().transition(InstanceInput::ActionCut(vec![kept.clone()]));
    assert_eq!(commands, [InstanceCommand::RunCleanup(kept)], "the cut action's own cleanup, newest, first");
    let (workflow, commands) = workflow.transition(InstanceInput::CleanupDone);
    assert_eq!(commands, [InstanceCommand::RunCleanup(PendingCleanup::new(sim::acts(&[100])[0]))]);
    let (workflow, _) = workflow.transition(InstanceInput::CleanupDone);
    assert_eq!(workflow.phase(), InstancePhase::Done(TestStatus::Skipped));
}

/// Fails the first attempt and returns the workflow waiting to retry.
fn waiting() -> InstanceWorkflow {
    let (workflow, _) = in_body().transition(InstanceInput::ActionDone(sim::fail(11)));
    let (workflow, _) = workflow.transition(InstanceInput::ActionDone(ActionReport::passed()));
    let (workflow, _) = workflow.transition(InstanceInput::ActionDone(ActionReport::passed()));
    let (workflow, commands) = workflow.transition(InstanceInput::CleanupDone);
    assert_eq!(commands, [InstanceCommand::Wait(Duration::from_millis(250))]);
    assert_eq!(workflow.phase(), InstancePhase::Waiting);
    workflow
}

#[test]
fn cancel_while_waiting_to_retry_keeps_the_failure() {
    let (workflow, commands) = waiting().transition(InstanceInput::Cancelled);
    assert_eq!(workflow.phase(), InstancePhase::Done(TestStatus::Failed));
    assert!(matches!(&commands[..], [InstanceCommand::Finish(v)]
        if v.attempts == 1 && v.failure.as_ref().and_then(|f| f.action).map(vc_model::ActionId::index) == Some(11)));
}

#[test]
fn a_cut_retry_reports_the_earlier_failure() {
    let (workflow, _) = waiting().transition(InstanceInput::RetryDue);
    let (workflow, commands) = workflow.transition(InstanceInput::ActionCut(Vec::new()));
    assert_eq!(workflow.phase(), InstancePhase::Done(TestStatus::Failed));
    assert!(matches!(&commands[..], [InstanceCommand::Finish(v)] if v.attempts == 2 && v.failure.is_some()));
}

#[test]
fn abort_abandons_cleanups_newest_first_and_finishes_at_once() {
    let (workflow, _) = in_body().transition(InstanceInput::ActionDone(sim::passing_with(&[101])));
    let (workflow, commands) = workflow.transition(InstanceInput::Abort);
    assert_eq!(commands[0], InstanceCommand::Abandon(sim::acts(&[101, 100])));
    assert!(matches!(&commands[1..], [InstanceCommand::Finish(v)] if v.status == TestStatus::Skipped));
    assert!(matches!(workflow.phase(), InstancePhase::Done(_)));
}

#[test]
fn abort_during_cleanups_keeps_the_verdict() {
    let (workflow, _) = in_body().transition(InstanceInput::ActionDone(sim::fail(11)));
    let (workflow, _) = workflow.transition(InstanceInput::ActionDone(ActionReport::passed()));
    let (workflow, commands) = workflow.transition(InstanceInput::ActionDone(ActionReport::passed()));
    assert!(matches!(&commands[..], [InstanceCommand::RunCleanup(_)]));
    let (workflow, commands) = workflow.transition(InstanceInput::Abort);
    assert_eq!(workflow.phase(), InstancePhase::Done(TestStatus::Failed));
    assert!(matches!(&commands[..], [InstanceCommand::Finish(_)]));
}

#[test]
fn inputs_out_of_place_are_ignored() {
    let pending = InstanceWorkflow::new(script(None));
    for input in
        [InstanceInput::ActionDone(ActionReport::passed()), InstanceInput::CleanupDone, InstanceInput::RetryDue]
    {
        let (workflow, commands) = pending.clone().transition(input);
        assert_eq!(commands.len(), 0);
        assert_eq!(workflow.phase(), InstancePhase::Pending);
    }
    let log = run(None, &[]);
    assert!(matches!(log.last(), Some(InstanceCommand::Finish(_))));
    let (done, _) = in_body().transition(InstanceInput::Abort);
    assert_eq!(done.transition(InstanceInput::Start).1.len(), 0);
}

#[test]
fn an_empty_script_passes_at_once() {
    let log = sim::drive_instance(InstanceWorkflow::new(Arc::default()), InstanceInput::Start, &mut failing(&[]));
    assert_eq!(sim::verdict(&log).status, TestStatus::Passed);
}
