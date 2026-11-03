use std::sync::Arc;

use vc_model::{Event, OwnerKey, Plan, PlannedFile, RunStatus, SkippedTest, TestStatus, Totals};

use super::super::sim::{act, drive_run, fail, file_and_test, instance, passing_with, planned, planned_file};
use super::super::{ActionReport, PendingCleanup};
use super::{RunCommand, RunInput, RunOrchestrator, RunPhase, RunSummary};

fn file() -> PlannedFile {
    (*planned_file(vec![], vec![instance(0, &[])], &[])).clone()
}

fn plan(before_run: &[(u32, &[u32])], files: usize, after_run: &[u32]) -> Arc<Plan> {
    Arc::new(Plan {
        before_run: before_run.iter().map(|&(action, needs)| planned(action, needs)).collect(),
        files: vec![file(); files],
        after_run: after_run.iter().copied().map(act).collect(),
        skipped: Vec::new(),
    })
}

fn passed(count: usize) -> Totals {
    Totals { passed: count, ..Totals::default() }
}

fn finish(log: &[RunCommand]) -> RunSummary {
    let summaries: Vec<_> = log
        .iter()
        .filter_map(|command| match command {
            RunCommand::Finish(summary) => Some(*summary),
            _ => None,
        })
        .collect();
    assert_eq!(summaries.len(), 1, "exactly one Finish in {log:?}");
    summaries[0]
}

fn position(log: &[RunCommand], wanted: &RunCommand) -> Option<usize> {
    log.iter().position(|command| command == wanted)
}

#[test]
fn before_run_then_files_then_after_run_then_cleanups_then_report() {
    let plan = plan(&[(1, &[]), (2, &[])], 2, &[3]);
    let mut respond = |action: vc_model::ActionId| match action.index() {
        1 => passing_with(&[100]),
        3 => passing_with(&[101]),
        _ => ActionReport::passed(),
    };
    let log = drive_run(RunOrchestrator::new(plan, false), &mut respond, &[passed(1), passed(1)]);
    let start_files = log.iter().position(|c| matches!(c, RunCommand::StartFiles(_)));
    assert!(position(&log, &RunCommand::RunAction(act(2))) < start_files);
    assert!(start_files < position(&log, &RunCommand::RunAction(act(3))));
    let first_cleanup = position(&log, &RunCommand::RunCleanup(PendingCleanup { action: act(101) }));
    assert!(position(&log, &RunCommand::RunAction(act(3))) < first_cleanup);
    assert!(first_cleanup < position(&log, &RunCommand::RunCleanup(PendingCleanup { action: act(100) })));
    let report = log.iter().position(|c| matches!(c, RunCommand::Report(_)));
    assert!(log.iter().position(|c| matches!(c, RunCommand::Emit(Event::RunFinished { .. }))) < report);
    assert_eq!(finish(&log), RunSummary { status: RunStatus::Passed, totals: passed(2) });
    assert!(matches!(log.first(), Some(RunCommand::Emit(Event::RunStarted { files: 2, instances: 2 }))));
}

#[test]
fn failed_before_run_actions_reach_the_files_and_block_dependants() {
    let plan = plan(&[(1, &[]), (2, &[1])], 1, &[]);
    let mut respond = |action: vc_model::ActionId| if action.index() == 1 { fail(1) } else { ActionReport::passed() };
    let log = drive_run(RunOrchestrator::new(plan, false), &mut respond, &[passed(1)]);
    assert_eq!(position(&log, &RunCommand::RunAction(act(2))), None);
    let failed = log.iter().find_map(|c| match c {
        RunCommand::StartFiles(failed) => Some(failed.keys().copied().collect::<Vec<_>>()),
        _ => None,
    });
    assert_eq!(failed, Some(vec![act(1), act(2)]));
}

#[test]
fn the_status_follows_the_totals() {
    let cases = [
        (Totals { failed: 1, ..passed(1) }, RunStatus::Failed),
        (Totals { blocked: 1, ..Totals::default() }, RunStatus::Failed),
        (Totals { flaky: 1, skipped: 1, ..passed(1) }, RunStatus::Passed),
        (Totals::default(), RunStatus::Passed),
    ];
    for (totals, status) in cases {
        let log = drive_run(RunOrchestrator::new(plan(&[], 1, &[]), false), &mut |_| ActionReport::passed(), &[totals]);
        assert_eq!(finish(&log), RunSummary { status, totals });
    }
}

#[test]
fn skipped_tests_are_reported_and_counted() {
    let (_, test) = file_and_test();
    let plan =
        Arc::new(Plan { skipped: vec![SkippedTest { test, reason: "flaky upstream".into() }], ..Plan::default() });
    let log = drive_run(RunOrchestrator::new(plan, false), &mut |_| ActionReport::passed(), &[]);
    assert!(log.iter().any(|c| matches!(c, RunCommand::Emit(Event::TestFinished { result })
        if result.status == TestStatus::Skipped && result.skip_reason.as_deref() == Some("flaky upstream"))));
    assert_eq!(finish(&log).totals.skipped, 1);
}

/// A started run with one file, past `beforeRun` (which registered cleanup 100).
fn running() -> RunOrchestrator {
    let (run, _) = RunOrchestrator::new(plan(&[(1, &[])], 1, &[2]), false).transition(RunInput::Start);
    let (run, commands) = run.transition(RunInput::ActionDone(passing_with(&[100])));
    assert!(matches!(&commands[..], [RunCommand::StartFiles(_)]));
    run
}

#[test]
fn the_first_interrupt_cancels_skips_after_run_and_still_cleans_up() {
    let (run, commands) = running().transition(RunInput::Interrupt);
    assert_eq!(commands, [RunCommand::Cancel]);
    let (run, commands) = run.transition(RunInput::FileDone(Totals { skipped: 1, ..Totals::default() }));
    assert_eq!(commands, [RunCommand::RunCleanup(PendingCleanup { action: act(100) })]);
    let (run, commands) = run.transition(RunInput::CleanupDone);
    assert!(matches!(
        &commands[..],
        [RunCommand::Emit(Event::RunFinished { status: RunStatus::Cancelled, .. }), RunCommand::Report(_)]
    ));
    let (run, commands) = run.transition(RunInput::Reported);
    assert_eq!(run.phase(), RunPhase::Finished(RunStatus::Cancelled));
    assert!(matches!(&commands[..], [RunCommand::Finish(_)]));
}

#[test]
fn the_second_interrupt_skips_the_grace_and_the_third_aborts() {
    let (run, _) = running().transition(RunInput::Interrupt);
    let (run, commands) = run.transition(RunInput::Interrupt);
    assert_eq!(commands, [RunCommand::SkipGrace]);
    let (run, commands) = run.transition(RunInput::Interrupt);
    let abandoned = Event::CleanupsAbandoned { owner: OwnerKey::Run, cleanups: vec![act(100)] };
    assert_eq!(commands, [RunCommand::Abort, RunCommand::Emit(abandoned)], "waits for the file to stop");
    let (run, commands) = run.transition(RunInput::FileDone(Totals::default()));
    assert_eq!(run.phase(), RunPhase::Finished(RunStatus::Cancelled));
    assert!(matches!(&commands[..], [RunCommand::Emit(Event::RunFinished { .. }), RunCommand::Finish(_)]));
    assert_eq!(run.transition(RunInput::Interrupt).1.len(), 0);
}

#[test]
fn an_interrupt_during_before_run_starts_the_files_so_they_report_skips() {
    let (run, _) = RunOrchestrator::new(plan(&[(1, &[]), (2, &[])], 1, &[]), false).transition(RunInput::Start);
    let (run, _) = run.transition(RunInput::Interrupt);
    let (run, commands) = run.transition(RunInput::ActionDone(ActionReport::passed()));
    assert!(matches!(&commands[..], [RunCommand::StartFiles(_)]));
    let (cut, commands) = run.transition(RunInput::ActionCut);
    assert!(commands.is_empty() && cut.phase() == RunPhase::Running);
}

#[test]
fn a_cut_lifecycle_action_moves_on() {
    let (run, _) = RunOrchestrator::new(plan(&[(1, &[])], 1, &[]), false).transition(RunInput::Start);
    let (run, commands) = run.transition(RunInput::ActionCut);
    assert!(matches!(&commands[..], [RunCommand::StartFiles(_)]));
    assert_eq!(run.phase(), RunPhase::Running);
    let (run, _) = running().transition(RunInput::FileDone(Totals::default()));
    let (run, commands) = run.transition(RunInput::ActionCut);
    assert_eq!(commands, [RunCommand::RunCleanup(PendingCleanup { action: act(100) })]);
    assert_eq!(run.phase(), RunPhase::CleaningUp);
}

#[test]
fn bail_cancels_on_the_first_failed_or_blocked_instance() {
    for status in [TestStatus::Failed, TestStatus::Blocked] {
        let (run, _) = RunOrchestrator::new(plan(&[], 1, &[]), true).transition(RunInput::Start);
        let (run, commands) = run.transition(RunInput::Tested(status));
        assert_eq!(commands, [RunCommand::Cancel]);
        assert_eq!(run.transition(RunInput::Tested(status)).1.len(), 0, "cancels once");
    }
    let (run, _) = RunOrchestrator::new(plan(&[], 1, &[]), false).transition(RunInput::Start);
    assert_eq!(run.transition(RunInput::Tested(TestStatus::Failed)).1.len(), 0);
    let (run, _) = RunOrchestrator::new(plan(&[], 1, &[]), true).transition(RunInput::Start);
    assert_eq!(run.transition(RunInput::Tested(TestStatus::Flaky)).1.len(), 0);
}

#[test]
fn an_abort_outside_the_files_finishes_at_once() {
    let (run, _) = RunOrchestrator::new(plan(&[(1, &[])], 1, &[]), false).transition(RunInput::Start);
    let (run, _) = run.transition(RunInput::Interrupt);
    let (run, _) = run.transition(RunInput::Interrupt);
    let (run, commands) = run.transition(RunInput::Interrupt);
    assert_eq!(run.phase(), RunPhase::Finished(RunStatus::Cancelled));
    assert!(matches!(commands.last(), Some(RunCommand::Finish(_))));
    let (reporting, _) = running().transition(RunInput::FileDone(Totals::default()));
    let (reporting, _) = reporting.transition(RunInput::ActionDone(ActionReport::passed()));
    let (reporting, _) = reporting.transition(RunInput::CleanupDone);
    assert_eq!(reporting.phase(), RunPhase::Reporting);
    let (done, _) = (0..3).fold((reporting, Vec::new()), |(run, _), _| run.transition(RunInput::Interrupt));
    assert_eq!(done.phase(), RunPhase::Finished(RunStatus::Cancelled));
}

#[test]
fn an_empty_plan_reports_and_finishes() {
    let log = drive_run(RunOrchestrator::new(Arc::default(), false), &mut |_| ActionReport::passed(), &[]);
    assert_eq!(finish(&log), RunSummary { status: RunStatus::Passed, totals: Totals::default() });
}

#[test]
fn inputs_out_of_place_are_ignored() {
    let planned = RunOrchestrator::new(plan(&[], 1, &[]), false);
    for input in [
        RunInput::ActionDone(ActionReport::passed()),
        RunInput::CleanupDone,
        RunInput::Reported,
        RunInput::FileDone(Totals::default()),
    ] {
        let (run, commands) = planned.clone().transition(input);
        assert_eq!(commands.len(), 0);
        assert_eq!(run.phase(), RunPhase::Planned);
    }
}
