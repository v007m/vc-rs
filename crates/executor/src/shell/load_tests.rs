use std::sync::Arc;
use std::time::Duration;

use vc_diagnostics::Code;
use vc_model::{ActionKind, Check, CheckKind, Loop, Matcher, Origin, Retry, StatusPattern, TemplateValue, TestId};

use super::{IterationSetup, ProgramIterations};
use crate::cancel::CancelTree;
use crate::fixture::{Fixture, Sent, Server, span};
use crate::{ActionExecutors, EventBus, ExecutorError, HostLimits, IterationReport, Iterations, PipelineParts};

/// Prepares the iterations of a plan of `before_all` and one instance of each of `tests`.
async fn prepare(
    fixture: Fixture,
    (before_all, tests): (&[vc_model::ActionId], &[TestId]),
    full_checks: bool,
) -> (Result<ProgramIterations, ExecutorError>, Sent) {
    let plan = fixture.plan(before_all, &tests.iter().map(|&test| (test, 1)).collect::<Vec<_>>());
    let (server, sent) = Server::new();
    let executors = ActionExecutors::standard(PipelineParts::new(server, HostLimits::spawn(64)));
    let setup = IterationSetup { bus: EventBus::spawn(), cancel: CancelTree::new(Duration::from_secs(5)), full_checks };
    (ProgramIterations::prepare(fixture.cx(executors), Arc::new(plan), setup).await, sent)
}

fn statuses(report: &IterationReport) -> Vec<Option<u16>> {
    report.calls.iter().map(|call| call.status).collect()
}

#[tokio::test]
async fn before_all_runs_once_each_iteration_reports_its_calls_and_finish_cleans_up() {
    let mut fixture = Fixture::new();
    let cleanup = fixture.call("/cleanup");
    let (seed, browse) = (fixture.creating("/seed", cleanup), fixture.call("/browse"));
    let api = fixture.api_mut(browse).id;
    let test = fixture.test(vec![browse], None);
    let (iterations, sent) = prepare(fixture, (&[seed], &[test]), false).await;
    let iterations = iterations.unwrap();
    let mut reports = Vec::new();
    for user in 1..=3 {
        reports.push(iterations.iterate(test, user).await);
    }
    iterations.finish().await;
    assert!(reports.iter().all(|report| report.passed && report.failure.is_none()));
    assert!(reports.iter().all(|report| statuses(report) == [Some(200)]));
    assert!(reports.iter().all(|report| report.calls[0].api == Some(api)));
    let counts = sent.counts();
    assert_eq!((counts["/seed"], counts["/browse"], counts["/cleanup"]), (1, 3, 1));
}

#[tokio::test]
async fn a_failing_iteration_reports_its_failure_and_status() {
    let mut fixture = Fixture::new();
    let fail = fixture.call("/fail");
    let api = fixture.api_mut(fail).id;
    let test = fixture.test(vec![fail], None);
    let (iterations, _sent) = prepare(fixture, (&[], &[test]), false).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert!(!report.passed);
    let failure = report.failure.clone().unwrap_or_default();
    assert_eq!((failure.api, failure.status, failure.code), (Some(api), Some(500), Some(Code::V5101.to_string())));
    assert_eq!(statuses(&report), [Some(500)]);
}

#[tokio::test]
async fn a_failed_check_on_the_second_call_blames_that_call_not_the_cleanup_after_it() {
    let mut fixture = Fixture::new();
    let cleanup = fixture.call("/cleanup");
    let (seed, browse) = (fixture.creating("/seed", cleanup), fixture.call("/browse"));
    let api = fixture.api_mut(browse).id;
    let test = fixture.test(vec![seed, browse], None);
    let matcher = Matcher::Status(vec![StatusPattern::Code(500)]);
    let check = Check { kind: CheckKind::Status, target: None, matcher, origin: Origin::Test(test), span: span() };
    fixture.action_mut(browse).expect.checks.push(check);
    let (iterations, _sent) = prepare(fixture, (&[], &[test]), true).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert_eq!(statuses(&report), [Some(200), Some(200), Some(200)], "the seed, the browse and its cleanup");
    let failure = report.failure.unwrap_or_default();
    assert_eq!((failure.api, failure.status, failure.code), (Some(api), Some(200), Some(Code::V5101.to_string())));
}

#[tokio::test]
async fn with_checks_api_a_test_check_is_not_judged_and_the_default_status_stays() {
    for (path, full, passed) in [("/browse", false, true), ("/browse", true, false)] {
        let mut fixture = Fixture::new();
        let call = fixture.call(path);
        let test = fixture.test(vec![call], None);
        let matcher = Matcher::Status(vec![StatusPattern::Code(500)]);
        let check = Check { kind: CheckKind::Status, target: None, matcher, origin: Origin::Test(test), span: span() };
        fixture.action_mut(call).expect.checks.push(check);
        let (iterations, _sent) = prepare(fixture, (&[], &[test]), full).await;
        let report = iterations.unwrap().iterate(test, 1).await;
        assert_eq!(report.passed, passed, "{path} with full checks {full}");
    }
}

#[tokio::test]
async fn with_checks_api_the_default_2xx_holds_unless_the_api_checks_the_status() {
    for (api_checks_500, passed) in [(false, false), (true, true)] {
        let mut fixture = Fixture::new();
        let call = fixture.call("/fail");
        let test = fixture.test(vec![call], None);
        if api_checks_500 {
            let origin = Origin::Api(fixture.api_mut(call).id);
            let matcher = Matcher::Status(vec![StatusPattern::Code(500)]);
            let check = Check { kind: CheckKind::Status, target: None, matcher, origin, span: span() };
            fixture.action_mut(call).expect.checks.push(check);
        }
        let (iterations, _sent) = prepare(fixture, (&[], &[test]), false).await;
        let report = iterations.unwrap().iterate(test, 1).await;
        assert_eq!(report.passed, passed, "a 500 when the api checks for 500: {api_checks_500}");
    }
}

#[tokio::test]
async fn a_failed_before_all_stops_the_load_run_and_cleans_up_what_ran() {
    let mut fixture = Fixture::new();
    let cleanup = fixture.call("/cleanup");
    let (seed, fail, browse) = (fixture.creating("/seed", cleanup), fixture.call("/fail"), fixture.call("/browse"));
    let test = fixture.test(vec![browse], None);
    let (iterations, sent) = prepare(fixture, (&[seed, fail], &[test]), false).await;
    assert!(matches!(iterations, Err(ExecutorError::SharedFailed { action, .. }) if action == fail));
    let counts = sent.counts();
    assert_eq!((counts["/seed"], counts["/fail"], counts["/cleanup"]), (1, 1, 1));
}

#[tokio::test]
async fn a_test_outside_the_plan_fails_without_a_request() {
    let mut fixture = Fixture::new();
    let (browse, other) = (fixture.call("/browse"), fixture.call("/other"));
    let (test, unplanned) = (fixture.test(vec![browse], None), fixture.test(vec![other], None));
    let (iterations, sent) = prepare(fixture, (&[], &[test]), false).await;
    let report = iterations.unwrap().iterate(unplanned, 1).await;
    assert!(!report.passed && report.calls.is_empty());
    let failure = report.failure.unwrap_or_default();
    assert_eq!(
        (failure.api, failure.status, failure.message.as_str()),
        (None, None, "the test is not in the load plan")
    );
    assert!(sent.counts().is_empty());
}

#[tokio::test]
async fn finish_runs_every_cleanup_an_owner_registered() {
    let mut fixture = Fixture::new();
    let (undo_a, undo_b) = (fixture.call("/cleanup/a"), fixture.call("/cleanup/b"));
    let seeds = [fixture.creating("/seed/a", undo_a), fixture.creating("/seed/b", undo_b)];
    let browse = fixture.call("/browse");
    let test = fixture.test(vec![browse], None);
    let (iterations, sent) = prepare(fixture, (&seeds, &[test]), false).await;
    iterations.unwrap().finish().await;
    let counts = sent.counts();
    assert_eq!((counts.get("/cleanup/a"), counts.get("/cleanup/b")), (Some(&1), Some(&1)), "{counts:?}");
}

#[tokio::test]
async fn an_iteration_reads_what_its_file_before_all_saved() {
    let mut fixture = Fixture::new();
    let seed = fixture.call("/seed");
    fixture.action_mut(seed).id = Some(fixture.sym("user"));
    fixture.action_mut(seed).save.values = fixture.vars(&[("token", "t")]);
    let read = fixture.call("/users");
    fixture.api_mut(read).path = fixture.template("/users/${var.user.token}");
    let test = fixture.test(vec![read], None);
    let (iterations, sent) = prepare(fixture, (&[seed], &[test]), false).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert!(report.passed, "{report:?}");
    let counts = sent.counts();
    assert_eq!((counts.get("/seed"), counts.get("/users/t")), (Some(&1), Some(&1)), "{counts:?}");
}

#[tokio::test]
async fn a_retried_iteration_blames_its_last_attempt_not_an_earlier_one_s_call() {
    let mut fixture = Fixture::new();
    let get = fixture.call("/sinking");
    let api = fixture.api_mut(get).id;
    let retry = Retry { attempts: 1, delay: vc_model::Duration::new(Duration::from_millis(10)) };
    let test = fixture.test(vec![get], Some(retry));
    let (iterations, sent) = prepare(fixture, (&[], &[test]), false).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert_eq!((statuses(&report), sent.counts()["/sinking"]), (vec![Some(503)], 2), "the refused call got nothing");
    let failure = report.failure.unwrap_or_default();
    assert_eq!((failure.api, failure.status, failure.code), (Some(api), None, Some(Code::V5105.to_string())));
}

#[tokio::test]
async fn a_failed_for_each_turn_is_blamed_with_its_own_status() {
    let mut fixture = Fixture::new();
    let get = fixture.call("/items");
    let api = fixture.api_mut(get).id;
    fixture.api_mut(get).path = fixture.template("/${item}");
    let items = ["missing", "a", "b"].map(|item| fixture.value(item));
    fixture.action_mut(get).repeat = Some(Loop::ForEach { items: TemplateValue::List(items.to_vec()), parallel: 3 });
    let test = fixture.test(vec![get], None);
    let (iterations, _sent) = prepare(fixture, (&[], &[test]), false).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert_eq!(statuses(&report), [Some(404), Some(200), Some(200)]);
    let failure = report.failure.unwrap_or_default();
    assert_eq!((failure.api, failure.status, failure.code), (Some(api), Some(404), Some(Code::V5101.to_string())));
}

#[tokio::test]
async fn a_failed_step_of_a_reused_test_is_blamed_with_its_status() {
    let mut fixture = Fixture::new();
    let (browse, create) = (fixture.call("/users"), fixture.call("/fail"));
    let api = fixture.api_mut(create).id;
    let reused = fixture.test(vec![browse, create], None);
    let reuse = fixture.action(ActionKind::Test(reused));
    let test = fixture.test(vec![reuse], None);
    let (iterations, _sent) = prepare(fixture, (&[], &[test]), false).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert_eq!(statuses(&report), [Some(200), Some(500)]);
    let failure = report.failure.unwrap_or_default();
    assert_eq!((failure.api, failure.status, failure.code), (Some(api), Some(500), Some(Code::V5101.to_string())));
}

#[tokio::test]
async fn a_failed_after_action_does_not_take_the_blame_from_the_body_s_failure() {
    let mut fixture = Fixture::new();
    let (fail, missing) = (fixture.call("/fail"), fixture.call("/missing"));
    let api = fixture.api_mut(fail).id;
    let test = fixture.test(vec![fail], None);
    fixture.test_mut(test).after = vec![missing];
    let (iterations, _sent) = prepare(fixture, (&[], &[test]), false).await;
    let report = iterations.unwrap().iterate(test, 1).await;
    assert_eq!(statuses(&report), [Some(500), Some(404)], "the body's call and the after's");
    let failure = report.failure.unwrap_or_default();
    assert_eq!((failure.api, failure.status, failure.code), (Some(api), Some(500), Some(Code::V5101.to_string())));
}
