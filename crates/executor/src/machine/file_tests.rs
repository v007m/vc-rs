use vc_model::{Event, Failure, SkippedTest, TestStatus, Totals};

use super::super::sim;
use super::super::{ActionReport, FailedActions, PendingCleanup};
use super::{FileCommand, FileInput, FileOrchestrator, FilePhase, Launch};

fn passing(_: vc_model::ActionId) -> ActionReport {
    ActionReport::passed()
}

fn launches(log: &[FileCommand]) -> Vec<&Launch> {
    log.iter()
        .filter_map(|command| match command {
            FileCommand::Launch(launch) => Some(launch),
            _ => None,
        })
        .collect()
}

fn finished(log: &[FileCommand]) -> Totals {
    let totals: Vec<_> = log
        .iter()
        .filter_map(|command| match command {
            FileCommand::Finish(totals) => Some(*totals),
            _ => None,
        })
        .collect();
    assert_eq!(totals.len(), 1, "exactly one Finish in {log:?}");
    totals[0]
}

#[test]
fn before_all_runs_first_then_instances_under_the_window_then_after_all() {
    let file = sim::planned_file(
        vec![sim::planned(1, &[]), sim::planned(2, &[])],
        (0..5).map(|i| sim::instance(i, &[])).collect(),
        &[3],
    );
    let trace = sim::drive_file(FileOrchestrator::new(file, 2), &mut passing, &mut |_| TestStatus::Passed);
    let first_launch = trace.log.iter().position(|c| matches!(c, FileCommand::Launch(_)));
    let actions: Vec<_> = trace.log.iter().filter(|c| matches!(c, FileCommand::RunAction(_))).collect();
    assert_eq!(
        actions,
        [
            &FileCommand::RunAction(sim::act(1)),
            &FileCommand::RunAction(sim::act(2)),
            &FileCommand::RunAction(sim::act(3))
        ]
    );
    let before_all_done = trace.log.iter().position(|c| *c == FileCommand::RunAction(sim::act(2)));
    assert!(before_all_done < first_launch);
    let after_all = trace.log.iter().position(|c| *c == FileCommand::RunAction(sim::act(3)));
    assert!(trace.log.iter().rposition(|c| matches!(c, FileCommand::Launch(_))) < after_all);
    assert_eq!(trace.max_running, 2);
    assert_eq!(launches(&trace.log).len(), 5);
    assert_eq!(finished(&trace.log).passed, 5);
    assert!(matches!(trace.log.first(), Some(FileCommand::Emit(Event::FileStarted { .. }))));
}

#[test]
fn a_failed_before_all_blocks_only_its_dependants() {
    let before_all = vec![sim::planned(1, &[]), sim::planned(2, &[1]), sim::planned(3, &[])];
    let file = sim::planned_file(
        before_all,
        vec![sim::instance(0, &[2]), sim::instance(1, &[3]), sim::instance(2, &[1])],
        &[],
    );
    let mut respond =
        |action: vc_model::ActionId| if action.index() == 1 { sim::fail(1) } else { ActionReport::passed() };
    let trace = sim::drive_file(FileOrchestrator::new(file, 4), &mut respond, &mut |launch: &Launch| {
        if launch.block.is_some() { TestStatus::Blocked } else { TestStatus::Passed }
    });
    assert!(!trace.log.contains(&FileCommand::RunAction(sim::act(2))), "action 2 needs the failed 1");
    assert!(
        trace
            .log
            .iter()
            .any(|c| matches!(c, FileCommand::Emit(Event::ActionFinished { action, .. }) if *action == sim::act(2)))
    );
    let blocks: Vec<_> = launches(&trace.log).iter().map(|l| l.block.as_ref().and_then(|f| f.action)).collect();
    assert_eq!(blocks, [Some(sim::act(1)), None, Some(sim::act(1))]);
    let totals = finished(&trace.log);
    assert_eq!((totals.blocked, totals.passed), (2, 1));
}

#[test]
fn run_level_failures_block_instances_and_before_all() {
    let file =
        sim::planned_file(vec![sim::planned(1, &[50])], vec![sim::instance(0, &[50]), sim::instance(1, &[])], &[]);
    let failed = FailedActions::from([(sim::act(50), Failure::new("beforeRun failed").at(sim::act(50)))]);
    let (file, commands) = FileOrchestrator::new(file, 4).transition(FileInput::Start(failed));
    assert!(!commands.contains(&FileCommand::RunAction(sim::act(1))));
    let blocks: Vec<_> = launches(&commands).iter().map(|l| l.block.is_some()).collect();
    assert_eq!(blocks, [true, false]);
    assert_eq!(file.phase(), FilePhase::Running);
}

#[test]
fn file_cleanups_run_newest_first_after_after_all() {
    let file = sim::planned_file(vec![sim::planned(1, &[]), sim::planned(2, &[])], vec![sim::instance(0, &[])], &[3]);
    let mut respond = |action: vc_model::ActionId| match action.index() {
        1 => sim::passing_with(&[100]),
        2 => sim::passing_with(&[101, 102]),
        _ => ActionReport::passed(),
    };
    let trace = sim::drive_file(FileOrchestrator::new(file, 1), &mut respond, &mut |_| TestStatus::Failed);
    let cleanups: Vec<_> = trace
        .log
        .iter()
        .filter_map(|c| match c {
            FileCommand::RunCleanup(cleanup) => Some(cleanup.action.index()),
            _ => None,
        })
        .collect();
    assert_eq!(cleanups, [102, 101, 100]);
    let after_all = trace.log.iter().position(|c| *c == FileCommand::RunAction(sim::act(3)));
    assert!(after_all < trace.log.iter().position(|c| matches!(c, FileCommand::RunCleanup(_))));
    assert_eq!(finished(&trace.log).failed, 1);
}

#[test]
fn an_empty_file_finishes_after_its_lifecycle() {
    let file = sim::planned_file(vec![sim::planned(1, &[])], vec![], &[2]);
    let trace = sim::drive_file(FileOrchestrator::new(file, 0), &mut passing, &mut |_| TestStatus::Passed);
    assert_eq!(finished(&trace.log), Totals::default());
    assert!(trace.log.contains(&FileCommand::RunAction(sim::act(2))));
}

/// A file with 4 instances and window 2, started: two instances are running.
fn running() -> FileOrchestrator {
    let file = sim::planned_file(vec![sim::planned(1, &[])], (0..4).map(|i| sim::instance(i, &[])).collect(), &[2]);
    let (file, _) = FileOrchestrator::new(file, 2).transition(FileInput::Start(FailedActions::new()));
    let (file, commands) = file.transition(FileInput::ActionDone(Box::new(sim::passing_with(&[100]))));
    assert_eq!(launches(&commands).len(), 2);
    file
}

#[test]
fn cancel_skips_unlaunched_instances_and_after_all_but_runs_cleanups() {
    let (file, commands) = running().transition(FileInput::Cancelled);
    assert_eq!(commands.len(), 0, "waits for the running instances");
    let (file, commands) = file.transition(FileInput::InstanceDone(TestStatus::Passed));
    assert_eq!(commands.len(), 0);
    let (file, commands) = file.transition(FileInput::InstanceDone(TestStatus::Skipped));
    let skipped = commands.iter().filter(|c| matches!(c, FileCommand::Emit(Event::TestFinished { .. }))).count();
    assert_eq!(skipped, 2);
    assert!(!commands.contains(&FileCommand::RunAction(sim::act(2))));
    assert!(commands.contains(&FileCommand::RunCleanup(PendingCleanup::new(sim::act(100)))));
    let (file, commands) = file.transition(FileInput::CleanupDone);
    assert_eq!(file.phase(), FilePhase::Done);
    let totals = finished(&commands);
    assert_eq!((totals.passed, totals.skipped), (1, 3));
}

#[test]
fn cancel_before_start_skips_every_instance() {
    let file = sim::planned_file(vec![sim::planned(1, &[])], (0..3).map(|i| sim::instance(i, &[])).collect(), &[]);
    let (file, commands) = FileOrchestrator::new(file, 2).transition(FileInput::Cancelled);
    assert_eq!(file.phase(), FilePhase::Done);
    assert!(!commands.iter().any(|c| matches!(c, FileCommand::RunAction(_) | FileCommand::Launch(_))));
    assert_eq!(finished(&commands).skipped, 3);
}

#[test]
fn cancel_during_before_all_stops_after_the_action_in_flight() {
    let file = sim::planned_file(vec![sim::planned(1, &[]), sim::planned(2, &[])], vec![sim::instance(0, &[])], &[]);
    let (file, _) = FileOrchestrator::new(file, 2).transition(FileInput::Start(FailedActions::new()));
    let (cancelled, _) = file.clone().transition(FileInput::Cancelled);
    let (done, commands) = cancelled.transition(FileInput::ActionDone(Box::new(sim::passing_with(&[100]))));
    assert!(!commands.contains(&FileCommand::RunAction(sim::act(2))));
    assert!(commands.contains(&FileCommand::RunCleanup(PendingCleanup::new(sim::act(100)))));
    let (_, commands) = done.transition(FileInput::CleanupDone);
    assert_eq!(finished(&commands).skipped, 1);
    let (cut, commands) = file.transition(FileInput::ActionCut(Vec::new()));
    assert_eq!(cut.phase(), FilePhase::Done);
    assert_eq!(finished(&commands).skipped, 1);
}

#[test]
fn abort_abandons_cleanups_and_finishes_when_instances_stop() {
    let (file, commands) = running().transition(FileInput::Abort);
    assert!(commands.iter().any(
        |c| matches!(c, FileCommand::Emit(Event::CleanupsAbandoned { cleanups, .. }) if *cleanups == [sim::act(100)])
    ));
    assert!(!commands.iter().any(|c| matches!(c, FileCommand::Finish(_))));
    let (file, _) = file.transition(FileInput::InstanceDone(TestStatus::Skipped));
    let (file, commands) = file.transition(FileInput::InstanceDone(TestStatus::Skipped));
    assert_eq!(file.phase(), FilePhase::Done);
    assert!(!commands.iter().any(|c| matches!(c, FileCommand::RunCleanup(_) | FileCommand::RunAction(_))));
    assert_eq!(finished(&commands).skipped, 4);
}

#[test]
fn abort_with_nothing_running_finishes_at_once() {
    let file = sim::planned_file(vec![], vec![sim::instance(0, &[])], &[]);
    let (file, commands) = FileOrchestrator::new(file, 1).transition(FileInput::Abort);
    assert_eq!(file.phase(), FilePhase::Done);
    assert_eq!(finished(&commands).skipped, 1);
}

#[test]
fn inputs_out_of_place_are_ignored() {
    let file = sim::planned_file(vec![], vec![sim::instance(0, &[])], &[]);
    let waiting = FileOrchestrator::new(file, 1);
    for input in [
        FileInput::ActionDone(Box::new(ActionReport::passed())),
        FileInput::CleanupDone,
        FileInput::InstanceDone(TestStatus::Passed),
    ] {
        let (file, commands) = waiting.clone().transition(input);
        assert_eq!(commands.len(), 0);
        assert_eq!(file.phase(), FilePhase::Waiting);
    }
    let (done, _) = waiting.transition(FileInput::Abort);
    assert_eq!(done.transition(FileInput::Start(FailedActions::new())).1.len(), 0);
}

#[test]
fn a_skipped_test_is_reported_after_the_file_starts_and_counted_in_its_totals() {
    let mut file = (*sim::planned_file(vec![], vec![sim::instance(0, &[])], &[])).clone();
    file.skipped = vec![SkippedTest { test: sim::file_and_test().1, reason: "flaky upstream".into() }];
    let file = std::sync::Arc::new(file);
    let trace = sim::drive_file(FileOrchestrator::new(file, 1), &mut passing, &mut |_| TestStatus::Passed);
    assert!(matches!(trace.log.first(), Some(FileCommand::Emit(Event::FileStarted { .. }))));
    assert!(matches!(trace.log.get(1), Some(FileCommand::Emit(Event::TestFinished { result }))
        if result.status == TestStatus::Skipped && result.skip_reason.as_deref() == Some("flaky upstream")));
    let totals = finished(&trace.log);
    assert_eq!((totals.passed, totals.skipped), (1, 1));
}
