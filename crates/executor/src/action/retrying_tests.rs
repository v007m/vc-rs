use std::sync::Arc;
use std::time::Instant;

use vc_interfaces::Transport;
use vc_model::{ActionId, ActionOutcome, Check, CheckKind, Duration, Matcher, Origin, OwnerKey, Retry, StatusPattern};
use vc_testkit::FakeTransport;

use crate::action::{ActionExecutors, ActionRequest};
use crate::fixture::{Fixture, Server};
use crate::{ActionReport, Frame, HostLimits, PendingCleanup, PipelineParts};

fn retry(attempts: u32, millis: u64) -> Retry {
    Retry { attempts, delay: Duration::new(std::time::Duration::from_millis(millis)) }
}

async fn execute(fixture: Fixture, transport: Arc<dyn Transport>, action: ActionId) -> ActionReport {
    let cx = fixture.cx(ActionExecutors::standard(PipelineParts::new(transport, HostLimits::spawn(8))));
    let request = ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
}

/// A call of `path` with `retry`.
fn retried(fixture: &mut Fixture, path: &str, attempts: u32) -> ActionId {
    let call = fixture.call(path);
    fixture.action_mut(call).retry = Some(retry(attempts, 0));
    call
}

#[tokio::test]
async fn a_failed_action_runs_again_after_its_delay_and_passes() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/flaky");
    fixture.action_mut(call).retry = Some(retry(2, 50));
    let (server, sent) = Server::new();
    let started = Instant::now();
    let report = execute(fixture, server, call).await;
    assert_eq!(report.outcome, ActionOutcome::Passed);
    assert_eq!(sent.counts().get("/flaky"), Some(&2), "a 503, then a 200 ends the retrying");
    assert!(started.elapsed() >= std::time::Duration::from_millis(50), "it waited out the delay");
}

#[tokio::test]
async fn an_action_that_passes_runs_once() {
    let mut fixture = Fixture::new();
    let call = retried(&mut fixture, "/users", 3);
    let (server, sent) = Server::new();
    assert_eq!(execute(fixture, server, call).await.outcome, ActionOutcome::Passed);
    assert_eq!(sent.counts().get("/users"), Some(&1));
}

#[tokio::test]
async fn the_last_failure_stands_once_the_attempts_run_out() {
    let mut fixture = Fixture::new();
    let call = retried(&mut fixture, "/fail", 2);
    let (server, sent) = Server::new();
    let report = execute(fixture, server, call).await;
    assert!(report.outcome.failure().is_some_and(|failure| failure.action == Some(call)));
    assert_eq!(sent.counts().get("/fail"), Some(&3), "the first attempt and two more");
}

#[tokio::test]
async fn an_action_without_retry_runs_once_and_fails() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/fail");
    let (server, sent) = Server::new();
    assert!(execute(fixture, server, call).await.outcome.failure().is_some());
    assert_eq!(sent.counts().get("/fail"), Some(&1));
}

/// The first create answers 200 where 201 is expected: it registers its cleanup and fails. The
/// cleanup takes the second response, so the retried create gets the 201.
#[tokio::test]
async fn the_failed_attempt_s_cleanups_run_before_the_next_attempt() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users/1");
    let create = fixture.creating("/users", delete);
    let span = crate::fixture::span();
    let matcher = Matcher::Status(vec![StatusPattern::Code(201)]);
    let check = Check { kind: CheckKind::Status, target: None, matcher, origin: Origin::Action(create), span };
    fixture.action_mut(create).expect.checks = vec![check];
    fixture.action_mut(create).retry = Some(retry(1, 0));
    let script = [200, 200, 201].map(|code| Ok(vc_testkit::status(code))).to_vec();
    let transport = FakeTransport::scripted(script);
    let report = execute(fixture, Arc::new(transport.clone()), create).await;
    assert_eq!(report.outcome, ActionOutcome::Passed);
    assert_eq!(report.cleanups, vec![PendingCleanup::new(delete)], "only the passing attempt's cleanup is left");
    assert_eq!(transport.calls(), 3);
}
