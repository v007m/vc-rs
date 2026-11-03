use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::{Instant, sleep_until};
use vc_model::{
    ActionId, ActionKind, Event, Feed, HookRef, OwnerKey, Plan, PlannedAction, Retry, RunStatus, TestResult,
    TestStatus, Totals,
};

use super::{Executor, RunOptions};
use crate::fixture::{Fixture, Server};
use crate::{
    ActionExecutors, ActionKindTag, EXIT_CANCELLED, EXIT_FAILED, EXIT_PASSED, EventBus, ExecutorError, HostLimits,
    PipelineParts, RunSummary, exit_code,
};

const SECOND: Duration = Duration::from_secs(1);

struct Outcome {
    summary: RunSummary,
    events: Vec<Event>,
    sent: BTreeMap<String, usize>,
    elapsed: Duration,
}

impl Outcome {
    fn sent(&self, path: &str) -> usize {
        self.sent.get(path).copied().unwrap_or(0)
    }

    fn results(&self) -> Vec<&TestResult> {
        self.events
            .iter()
            .filter_map(|event| match event {
                Event::TestFinished { result } => Some(result),
                _ => None,
            })
            .collect()
    }

    fn cleaned(&self) -> Vec<ActionId> {
        self.events
            .iter()
            .filter_map(|event| match event {
                Event::CleanupFinished { cleanup } => Some(cleanup.action),
                _ => None,
            })
            .collect()
    }
}

fn executor(fixture: Fixture, options: RunOptions) -> (Executor, crate::fixture::Sent) {
    let (server, sent) = Server::new();
    let executors = ActionExecutors::standard(PipelineParts::new(server, HostLimits::spawn(64)));
    (Executor::new(Arc::new(fixture.program), Arc::new(executors), options), sent)
}

/// Runs `plan`, interrupting at each of `interrupts` (from the start), and collects every event.
async fn run(fixture: Fixture, plan: Plan, options: RunOptions, interrupts: &[Duration]) -> Outcome {
    let (executor, sent) = executor(fixture, options);
    let bus = EventBus::spawn();
    let mut feed = bus.subscribe(100_000);
    let start = Instant::now();
    let handle = executor.start(Arc::new(plan), bus.clone()).unwrap();
    for &at in interrupts {
        sleep_until(start + at).await;
        handle.interrupt();
    }
    let summary = handle.finish().await.unwrap();
    let elapsed = start.elapsed();
    bus.flush().await;
    let feeds: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok()).collect();
    let mut seqs = Vec::new();
    let mut events = Vec::new();
    for feed in feeds {
        let Feed::Event(sequenced) = feed else { unreachable!("nothing is missed: {feed:?}") };
        seqs.push(sequenced.seq);
        events.push(sequenced.event);
    }
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>(), "run-wide numbers, in order");
    Outcome { summary, events, sent: sent.counts(), elapsed }
}

fn totals([passed, failed, blocked, flaky, skipped]: [usize; 5]) -> Totals {
    Totals { passed, failed, blocked, flaky, skipped }
}

#[tokio::test(start_paused = true)]
async fn fifty_instances_share_one_before_all_call() {
    let mut fixture = Fixture::new();
    let (setup, get) = (fixture.call("/setup"), fixture.call("/users"));
    let test = fixture.test(vec![get], None);
    let file = fixture.file;
    let plan = fixture.plan(&[setup], &[(test, 50)]);
    let outcome = run(fixture, plan, RunOptions { concurrency: 8, ..RunOptions::default() }, &[]).await;
    assert_eq!(outcome.summary, RunSummary { status: RunStatus::Passed, totals: totals([50, 0, 0, 0, 0]) });
    assert_eq!(exit_code(outcome.summary), EXIT_PASSED);
    assert_eq!((outcome.sent("/setup"), outcome.sent("/users")), (1, 50));
    let started = Event::ActionStarted { action: setup, owner: OwnerKey::File(file) };
    assert_eq!(outcome.events.iter().filter(|event| **event == started).count(), 1);
    assert_eq!(outcome.events.first(), Some(&Event::RunStarted { files: 1, instances: 50 }));
    assert!(matches!(outcome.events.last(), Some(Event::RunFinished { status: RunStatus::Passed, .. })));
}

#[tokio::test(start_paused = true)]
async fn a_failed_before_all_blocks_every_instance_that_needs_it() {
    let mut fixture = Fixture::new();
    let (setup, get) = (fixture.call("/fail"), fixture.call("/users"));
    let test = fixture.test(vec![get], None);
    let plan = fixture.plan(&[setup], &[(test, 50)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary, RunSummary { status: RunStatus::Failed, totals: totals([0, 0, 50, 0, 0]) });
    assert_eq!(exit_code(outcome.summary), EXIT_FAILED);
    assert_eq!((outcome.sent("/fail"), outcome.sent("/users")), (1, 0));
}

#[tokio::test(start_paused = true)]
async fn cleanups_run_newest_first_when_the_test_fails() {
    let mut fixture = Fixture::new();
    let (first, second) = (fixture.call("/users/1"), fixture.call("/users/2"));
    let steps =
        vec![fixture.creating("/create/1", first), fixture.creating("/create/2", second), fixture.call("/fail")];
    let test = fixture.test(steps, None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([0, 1, 0, 0, 0]));
    assert_eq!(outcome.cleaned(), [second, first]);
    let failure = outcome.results()[0].failure.clone().map(|failure| failure.message);
    assert_eq!(failure.as_deref(), Some("expected a 2xx status, got 500 Internal Server Error"));
}

#[tokio::test(start_paused = true)]
async fn cleanups_run_when_the_run_is_cancelled() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/users/1");
    let steps = vec![fixture.creating("/create/1", delete), fixture.call("/slow"), fixture.call("/never")];
    let test = fixture.test(steps, None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[SECOND]).await;
    assert_eq!(outcome.summary, RunSummary { status: RunStatus::Cancelled, totals: totals([0, 0, 0, 0, 1]) });
    assert_eq!(exit_code(outcome.summary), EXIT_CANCELLED);
    assert_eq!(outcome.cleaned(), [delete]);
    assert_eq!(outcome.sent("/never"), 0);
    assert_eq!(outcome.elapsed, 6 * SECOND, "the slow call had the 5 s grace period");
}

#[tokio::test(start_paused = true)]
async fn a_test_that_passes_on_retry_is_flaky() {
    let mut fixture = Fixture::new();
    let flaky = fixture.call("/flaky");
    let test = fixture.test(vec![flaky], Some(Retry { attempts: 2, delay: vc_model::Duration::new(SECOND) }));
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary, RunSummary { status: RunStatus::Passed, totals: totals([0, 0, 0, 1, 0]) });
    let result = outcome.results()[0];
    assert_eq!((result.status, result.attempts, outcome.sent("/flaky")), (TestStatus::Flaky, 2, 2));
    assert_eq!(outcome.elapsed, SECOND, "the retry waited its delay");
}

#[tokio::test(start_paused = true)]
async fn cancelling_during_a_retry_wait_gives_up_with_the_failure() {
    let mut fixture = Fixture::new();
    let fail = fixture.call("/fail");
    let test = fixture.test(vec![fail], Some(Retry { attempts: 3, delay: vc_model::Duration::new(10 * SECOND) }));
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[SECOND]).await;
    assert_eq!(outcome.summary, RunSummary { status: RunStatus::Cancelled, totals: totals([0, 1, 0, 0, 0]) });
    assert_eq!((outcome.sent("/fail"), outcome.elapsed), (1, SECOND));
}

#[tokio::test(start_paused = true)]
async fn a_third_interrupt_abandons_the_cleanups_left() {
    let mut fixture = Fixture::new();
    let (first, second) = (fixture.call("/slow"), fixture.call("/slow"));
    let steps =
        vec![fixture.creating("/create/1", first), fixture.creating("/create/2", second), fixture.call("/slow")];
    let test = fixture.test(steps, None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[SECOND, 2 * SECOND, 3 * SECOND]).await;
    assert_eq!(outcome.summary.status, RunStatus::Cancelled);
    let abandoned =
        Event::CleanupsAbandoned { owner: OwnerKey::Instance(vc_model::InstanceId::new(0)), cleanups: vec![first] };
    assert!(outcome.events.contains(&abandoned), "{:?}", outcome.events);
    assert_eq!((outcome.cleaned(), outcome.elapsed), (vec![], 3 * SECOND), "the cleanup in flight was stopped");
}

#[tokio::test(start_paused = true)]
async fn bail_stops_starting_tests_after_the_first_failure() {
    let mut fixture = Fixture::new();
    let fail = fixture.call("/fail");
    let test = fixture.test(vec![fail], None);
    let plan = fixture.plan(&[], &[(test, 3)]);
    let outcome = run(fixture, plan, RunOptions { concurrency: 1, bail: true, ..RunOptions::default() }, &[]).await;
    let Totals { failed, skipped, .. } = outcome.summary.totals;
    assert_eq!((outcome.summary.status, failed + skipped), (RunStatus::Failed, 3));
    assert!(skipped >= 1, "{:?}", outcome.summary);
}

#[tokio::test(start_paused = true)]
async fn each_instance_runs_the_file_before_each_and_after_each() {
    let mut fixture = Fixture::new();
    let (before, after, get) = (fixture.call("/before"), fixture.call("/after"), fixture.call("/users"));
    fixture.lifecycle(|blocks| (blocks.before_each, blocks.after_each) = (vec![before], vec![after]));
    let test = fixture.test(vec![get], None);
    let plan = fixture.plan(&[], &[(test, 3)]);
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([3, 0, 0, 0, 0]));
    assert_eq!((outcome.sent("/before"), outcome.sent("/users"), outcome.sent("/after")), (3, 3, 3));
}

#[tokio::test(start_paused = true)]
async fn run_lifecycle_actions_run_once_and_their_cleanups_run_last() {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/session/end");
    let (login, report, get) = (fixture.creating("/session", delete), fixture.call("/report"), fixture.call("/users"));
    let test = fixture.test(vec![get], None);
    let mut plan = fixture.plan(&[], &[(test, 4)]);
    plan.before_run = vec![PlannedAction { action: login, needs: vc_model::Needs::default() }];
    plan.after_run = vec![report];
    let outcome = run(fixture, plan, RunOptions::default(), &[]).await;
    assert_eq!(outcome.summary.totals, totals([4, 0, 0, 0, 0]));
    assert_eq!((outcome.sent("/session"), outcome.sent("/report"), outcome.sent("/session/end")), (1, 1, 1));
    assert_eq!(outcome.cleaned(), [delete]);
    let started = Event::ActionStarted { action: login, owner: OwnerKey::Run };
    assert!(outcome.events.contains(&started));
}

#[tokio::test]
async fn a_plan_that_reaches_what_this_build_cannot_run_does_not_start() {
    let mut fixture = Fixture::new();
    let hook = fixture.action(ActionKind::Function(HookRef { file: "hooks.ts".into(), export: "seed".into() }));
    let test = fixture.test(vec![hook], None);
    let plan = fixture.plan(&[], &[(test, 1)]);
    let mut unknown = plan.clone();
    let mut stranger = Fixture::new();
    stranger.test(vec![], None);
    let other = stranger.test(vec![], None);
    unknown.files[0].instances[0].test = other;
    let (executor, sent) = executor(fixture, RunOptions::default());
    let rejected = executor.start(Arc::new(plan), EventBus::spawn()).map(|_| ());
    assert_eq!(rejected, Err(ExecutorError::NoExecutor { action: hook, kind: ActionKindTag::Function }));
    let missing = executor.start(Arc::new(unknown), EventBus::spawn()).map(|_| ());
    assert_eq!(missing, Err(ExecutorError::UnknownTest(other)));
    assert_eq!(sent.counts().len(), 0);
    assert!(format!("{executor:?}").contains("concurrency: 16"));
}
