use std::sync::Arc;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use indexmap::IndexMap;
use tokio::sync::mpsc;
use vc_executor::{CancelTree, IterationReport, Iterations, MatcherRegistry};
use vc_model::{Duration, FileId, LoadScenario, LoadStage, Matcher, MixEntry, StringTable, TestId};
use vc_testkit::ScriptedRandom;

use super::{Ending, LoadOutcome, LoadParts, Second, run_load};
use crate::LoadError;
use crate::test_support::{Literals, number, thresholds};
use crate::user::think;

/// Every iteration takes `took` and passes or fails as `passed` says.
struct Takes {
    took: StdDuration,
    passed: bool,
}

#[async_trait]
impl Iterations for Takes {
    async fn iterate(&self, _test: TestId, _user: u32) -> IterationReport {
        tokio::time::sleep(self.took).await;
        IterationReport { passed: self.passed, ..IterationReport::default() }
    }
}

fn takes(millis: u64, passed: bool) -> Takes {
    Takes { took: StdDuration::from_millis(millis), passed }
}

fn stage(seconds: u64, users: u32) -> LoadStage {
    LoadStage { duration: Duration::new(StdDuration::from_secs(seconds)), users }
}

/// One test of weight 1, no think time, `limits` as the scenario's thresholds, aborting on
/// `abort_on`.
fn scenario(stages: Vec<LoadStage>, limits: IndexMap<String, Matcher>, abort_on: &[&str]) -> LoadScenario {
    let mut strings = StringTable::new();
    let test = TestId::new(strings.intern("a.f#browses").unwrap());
    let zero = Duration::new(StdDuration::ZERO);
    LoadScenario {
        id: FileId::new(strings.intern("a.peak").unwrap()),
        name: None,
        mix: vec![MixEntry { test, weight: 1, thresholds: IndexMap::new() }],
        stages,
        think_time: (zero, zero),
        full_checks: false,
        thresholds: limits,
        abort_on: abort_on.iter().map(|metric| (*metric).to_owned()).collect(),
    }
}

/// What a run did: its outcome, every second it sent and how many failed iterations it passed on.
type Ran = (Result<LoadOutcome, LoadError>, Vec<Second>, u64);

/// Runs `scenario` with `iterations` under `cancel`.
async fn run(scenario: &LoadScenario, iterations: Takes, cancel: CancelTree) -> Ran {
    let (seconds, mut sent) = mpsc::channel(64);
    let (failed, mut failures) = mpsc::unbounded_channel();
    let random = Arc::new(ScriptedRandom::seeded(7));
    let parts = LoadParts { iterations: Arc::new(iterations), random, cancel, seconds, failed };
    let registry = MatcherRegistry::standard();
    let outcome = run_load(scenario, parts, (&registry, &Literals)).await;
    let mut all = Vec::new();
    while let Some(second) = sent.recv().await {
        all.push(second);
    }
    let mut told = 0;
    while let Some((_test, report)) = failures.recv().await {
        assert!(!report.passed, "only failed iterations are passed on");
        told += 1;
    }
    (outcome, all, told)
}

fn root() -> CancelTree {
    CancelTree::new(StdDuration::from_secs(5))
}

/// A second sent, as (at, users, the (second, iterations) of its rows).
type Sent = (u64, u32, Vec<(u64, u64)>);

/// Each second sent.
fn seconds(sent: &[Second]) -> Vec<Sent> {
    sent.iter()
        .map(|second| {
            (second.at, second.users, second.rows.iter().map(|row| (row.second, row.tally.count())).collect())
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn users_follow_the_stages_and_every_iteration_lands_in_the_second_it_ended() {
    let scenario = scenario(vec![stage(2, 2), stage(2, 2)], IndexMap::new(), &[]);
    let (outcome, sent, told) = run(&scenario, takes(350, true), root()).await;
    let outcome = outcome.unwrap();
    assert_eq!(told, 0);
    assert_eq!(
        seconds(&sent),
        [(1, 0, vec![]), (2, 1, vec![(1, 2)]), (3, 2, vec![(2, 5)]), (4, 2, vec![(3, 6)]), (4, 0, vec![(4, 2)])]
    );
    assert_eq!(outcome.ending, Ending::Finished);
    assert_eq!((outcome.totals.all().count(), outcome.totals.seconds()), (15, 4));
    assert!((outcome.totals.all().latencies().quantile_ms(0.5) - 350.0).abs() < 1.0);
    assert_eq!(outcome.breaches, vec![]);
}

#[tokio::test(start_paused = true)]
async fn a_rolling_breach_of_an_abort_on_metric_cancels_the_run() {
    let limits = thresholds(&[("errorRate", number("lt", 0.01))]);
    let scenario = scenario(vec![stage(1, 2), stage(10, 2)], limits, &["errorRate"]);
    let cancel = root();
    let (outcome, sent, told) = run(&scenario, takes(350, false), cancel.clone()).await;
    let outcome = outcome.unwrap();
    let breaching: Vec<Vec<&str>> =
        sent.iter().map(|second| second.breaching.iter().map(|breach| breach.metric.as_str()).collect()).collect();
    assert_eq!(breaching, [vec![], vec!["errorRate"], vec![]]);
    assert_eq!((outcome.ending, outcome.totals.seconds()), (Ending::Aborted, 2));
    assert_eq!(told, outcome.totals.all().errors(), "an iteration dropped by the abort is not passed on");
    assert_eq!(outcome.breaches.iter().map(|breach| breach.metric.as_str()).collect::<Vec<_>>(), ["errorRate"]);
    assert!(cancel.is_cancelled());
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_run_stops_and_drops_the_iterations_that_end_after_it() {
    let scenario = scenario(vec![stage(1, 1), stage(10, 1)], IndexMap::new(), &[]);
    let cancel = root();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(StdDuration::from_millis(1500)).await;
        canceller.cancel();
    });
    let (outcome, _, _) = run(&scenario, takes(350, true), cancel).await;
    let outcome = outcome.unwrap();
    assert_eq!(outcome.ending, Ending::Cancelled);
    assert_eq!((outcome.totals.all().count(), outcome.totals.seconds()), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn only_the_failed_iterations_the_run_kept_are_passed_on() {
    let scenario = scenario(vec![stage(1, 1), stage(10, 1)], IndexMap::new(), &[]);
    let cancel = root();
    let canceller = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(StdDuration::from_millis(1500)).await;
        canceller.cancel();
    });
    let (outcome, _, told) = run(&scenario, takes(350, false), cancel).await;
    let outcome = outcome.unwrap();
    assert_eq!(outcome.ending, Ending::Cancelled);
    assert_eq!((told, outcome.totals.all().errors()), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn think_time_is_not_latency() {
    let mut scenario = scenario(vec![stage(1, 1), stage(2, 1)], IndexMap::new(), &[]);
    let pause = Duration::new(StdDuration::from_millis(100));
    scenario.think_time = (pause, pause);
    let (outcome, _, _) = run(&scenario, takes(200, true), root()).await;
    let totals = outcome.unwrap().totals;
    assert_eq!(totals.all().count(), 7);
    assert!((totals.all().latencies().max_ms() - 200.0).abs() < 1.0);
}

#[tokio::test(start_paused = true)]
async fn a_mix_that_cannot_pick_cancels_the_run_and_says_why() {
    let mut scenario = scenario(vec![stage(1, 1)], IndexMap::new(), &[]);
    scenario.mix[0].weight = 0;
    let cancel = root();
    let (outcome, sent, _) = run(&scenario, takes(1, true), cancel.clone()).await;
    assert_eq!(outcome.err(), Some(LoadError::EmptyMix));
    assert!(sent.is_empty());
    assert!(cancel.is_cancelled());
}

#[test]
fn think_time_is_drawn_between_its_bounds() {
    let (min, max) = (StdDuration::from_millis(10), StdDuration::from_millis(20));
    assert_eq!(think((min, max), 0), min);
    assert_eq!(think((min, max), 10_000_000), max);
    assert_eq!(think((min, max), 10_000_001), min);
    assert_eq!(think((min, min), u64::MAX), min);
    assert_eq!(think((max, min), 5), max);
}
