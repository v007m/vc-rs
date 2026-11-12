//! An `after` action carries no deadline of the shell's: each kind enforces its own `timeout`
//! (D192, D324).

use std::time::Duration;

use vc_diagnostics::Code;
use vc_model::{ActionId, Check, CheckKind, Expect, HttpRetry, Matcher, Origin, Poll, StatusPattern};

use super::RunOptions;
use super::tests::{Outcome, run, totals};
use crate::fixture::{self, Fixture};

const SECOND: Duration = Duration::from_secs(1);

fn secs(secs: u64) -> vc_model::Duration {
    vc_model::Duration::new(Duration::from_secs(secs))
}

/// Runs a test of one passing step with `after` as its `after` action.
async fn with_after(mut fixture: Fixture, after: ActionId) -> Outcome {
    let step = fixture.call("/users");
    let test = fixture.test(vec![step], None);
    fixture.test_mut(test).after = vec![after];
    let plan = fixture.plan(&[], &[(test, 1)]);
    run(fixture, plan, RunOptions::default(), &[]).await
}

/// The test's failure: its code and message.
fn failure(outcome: &Outcome) -> Option<(Option<String>, String)> {
    let failure = outcome.results().first()?.failure.as_ref()?;
    Some((failure.code.clone(), failure.message.clone()))
}

#[tokio::test(start_paused = true)]
async fn an_after_call_s_timeout_is_per_attempt_so_its_http_retry_runs() {
    let mut fixture = Fixture::new();
    let slow = fixture.call("/slow");
    fixture.action_mut(slow).timeout = Some(secs(2));
    let retry = HttpRetry { attempts: 1, base: secs(1), jitter: false, ..HttpRetry::default() };
    fixture.api_mut(slow).http_retry = Some(retry);
    let outcome = with_after(fixture, slow).await;
    assert_eq!(outcome.summary.totals, totals([0, 1, 0, 0, 0]));
    assert_eq!((outcome.sent("/slow"), outcome.elapsed), (2, 5 * SECOND), "2 s, the 1 s backoff, then 2 s");
    let (code, message) = failure(&outcome).unwrap_or_default();
    assert_eq!(code, Some(Code::V5104.to_string()));
    assert!(!message.contains("ran past its own"), "the request's own timeout, not a whole-action one: {message}");
}

#[tokio::test(start_paused = true)]
async fn an_after_call_s_poll_stops_at_its_poll_timeout_not_the_call_s() {
    let mut fixture = Fixture::new();
    let jobs = fixture.call("/fail");
    let matcher = Matcher::Status(vec![StatusPattern::Code(200)]);
    let ok =
        Check { kind: CheckKind::Status, target: None, matcher, origin: Origin::Action(jobs), span: fixture::span() };
    let until = Expect { checks: vec![ok], replace: false };
    fixture.action_mut(jobs).timeout = Some(secs(1));
    fixture.action_mut(jobs).poll = Some(Poll { until, every: secs(1), timeout: secs(3) });
    let outcome = with_after(fixture, jobs).await;
    assert_eq!(outcome.summary.totals, totals([0, 1, 0, 0, 0]));
    assert_eq!((outcome.sent("/fail"), outcome.elapsed), (4, 3 * SECOND), "sent at 0, 1, 2 and 3 s");
    let (code, message) = failure(&outcome).unwrap_or_default();
    assert_eq!(code, Some(Code::V5104.to_string()));
    assert!(message.starts_with("`poll.until` did not hold within 3s"), "{message}");
}
