use std::sync::Arc;
use std::time::Duration;

use vc_model::{ActionId, Event, OwnerKey, TestStatus};

use super::*;
use crate::testing::{self, Ticks};
use crate::text;

fn collected() -> Collector {
    let (program, plan) = testing::project();
    let mut collector = Collector::new(Arc::new(Ticks::default()));
    for event in testing::script(&program, &plan) {
        collector.take(&event);
    }
    collector
}

#[test]
fn gathers_tests_with_their_steps_in_finish_order() {
    let collector = collected();
    let statuses: Vec<TestStatus> = collector.tests.iter().map(|t| t.result.status).collect();
    let expected = [
        TestStatus::Passed,
        TestStatus::Failed,
        TestStatus::Blocked,
        TestStatus::Flaky,
        TestStatus::Passed,
        TestStatus::Skipped,
    ];
    assert_eq!(statuses, expected);
    let steps: Vec<usize> = collector.tests.iter().map(|t| t.steps.len()).collect();
    assert_eq!(steps, [1, 1, 0, 2, 1, 0]);
    let failed: Vec<usize> = collector.tests.iter().map(|t| t.failed_checks().count()).collect();
    assert_eq!(failed, [0, 2, 0, 1, 0, 0]);
    assert_eq!(collector.tests[1].steps[0].checks.len(), 2);
}

/// D305: the stopwatch places each instance at its first action; its duration is its result's.
#[test]
fn places_instances_at_their_first_action_and_times_them_by_their_result() {
    let collector = collected();
    let starts: Vec<Option<u64>> = collector.tests.iter().map(|t| t.start.map(text::millis)).collect();
    assert_eq!(starts, [Some(0), Some(150), None, Some(300), Some(600), None]);
    let durations: Vec<u64> = collector.tests.iter().map(|t| text::millis(t.duration())).collect();
    assert_eq!(durations, [150, 150, 0, 300, 150, 0]);
}

#[test]
fn keeps_files_lifecycle_and_cleanups_apart_from_tests() {
    let collector = collected();
    assert_eq!(collector.files.len(), 1);
    assert_eq!(collector.lifecycle.len(), 1);
    assert_eq!(collector.lifecycle[0].owner, OwnerKey::File(collector.files[0]));
    assert_eq!(collector.cleanups.len(), 1);
    assert_eq!(collector.abandoned, []);
}

#[test]
fn records_abandoned_cleanups_and_checks_of_lifecycle_actions() {
    let (program, _) = testing::project();
    let mut collector = Collector::new(Arc::new(Ticks::default()));
    let test = *program.tests.keys().next().unwrap();
    let check = Box::new(testing::sample_check(test));
    let (action, owner) = (ActionId::new(4), OwnerKey::Run);
    collector.take(&Event::CheckEvaluated { action, owner, check });
    collector.take(&Event::ActionFinished { action, owner, outcome: vc_model::ActionOutcome::Passed });
    collector.take(&Event::CleanupsAbandoned { owner, cleanups: vec![ActionId::new(1), ActionId::new(2)] });
    assert_eq!(collector.lifecycle[0].checks.len(), 1);
    assert_eq!(collector.abandoned, [(owner, ActionId::new(1)), (owner, ActionId::new(2))]);
    assert_eq!(collector.tests, []);
}

#[test]
fn a_record_that_never_started_lasts_no_time() {
    let collector = collected();
    assert_eq!(collector.tests[2].duration(), Duration::ZERO);
}
