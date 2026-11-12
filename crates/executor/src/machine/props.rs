//! Invariants of the machines under random scripts, outcomes, interrupts and input sequences.

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use proptest::prelude::{Just, Strategy, prop, prop_assert, prop_assert_eq, prop_oneof, proptest};
use vc_model::{ActionId, ActionOutcome, Event, Plan, Retry, TestStatus, Totals};

use super::sim;
use super::{
    ActionReport, FileCommand, FileEnd, FileInput, FileOrchestrator, InstanceCommand, InstanceInput, InstanceScript,
    InstanceWorkflow, PendingCleanup, RunCommand, RunInput, RunOrchestrator,
};

#[derive(Clone, Copy, Debug)]
enum Answer {
    Pass,
    Fail,
    Register(u32),
}

fn answers() -> impl Strategy<Value = Vec<Answer>> {
    let answer =
        prop_oneof![3 => Just(Answer::Pass), 2 => Just(Answer::Fail), 2 => (1u32..3).prop_map(Answer::Register)];
    prop::collection::vec(answer, 1..24)
}

/// Answers actions from `answers` in turn, giving each registered cleanup a fresh ID.
struct Responder {
    answers: Vec<Answer>,
    calls: usize,
    registered: Vec<u32>,
    failed: BTreeSet<ActionId>,
}

impl Responder {
    fn new(answers: Vec<Answer>) -> Self {
        Self { answers, calls: 0, registered: Vec::new(), failed: BTreeSet::new() }
    }

    fn respond(&mut self, action: ActionId) -> ActionReport {
        let answer = self.answers[self.calls % self.answers.len()];
        self.calls += 1;
        match answer {
            Answer::Pass => ActionReport::passed(),
            Answer::Fail => {
                self.failed.insert(action);
                sim::fail(action.index())
            }
            Answer::Register(count) => {
                let first = 1000 + u32::try_from(self.registered.len()).unwrap();
                let ids: Vec<u32> = (first..first + count).collect();
                self.registered.extend(&ids);
                sim::passing_with(&ids)
            }
        }
    }
}

/// Drives an instance like `sim::drive_instance`, with an optional interrupt `(step, abort)`, and
/// watches for a retry after a blocked attempt (a failed `before` action, IDs below 10).
struct Probe {
    responder: Responder,
    log: Vec<InstanceCommand>,
    /// The cleanups answered `CleanupDone`; an abort leaves the one in flight unanswered.
    finished: Vec<u32>,
    blocked_attempt: bool,
    retried_blocked: bool,
}

impl Probe {
    fn new(answers: Vec<Answer>) -> Self {
        let (log, finished) = (Vec::new(), Vec::new());
        Self { responder: Responder::new(answers), log, finished, blocked_attempt: false, retried_blocked: false }
    }

    fn run(&mut self, workflow: InstanceWorkflow, interrupt: Option<(usize, bool)>) {
        let (mut workflow, commands) = workflow.transition(InstanceInput::Start);
        let mut pending = VecDeque::from(commands);
        for step in 0..10_000 {
            if let Some((_, abort)) = interrupt.filter(|&(at, _)| at == step) {
                if abort {
                    self.log.extend(std::mem::take(&mut pending));
                }
                let (next, commands) =
                    workflow.transition(if abort { InstanceInput::Abort } else { InstanceInput::Cancelled });
                workflow = next;
                pending.extend(commands);
            }
            let Some(command) = pending.pop_front() else { break };
            let input = self.answer(&command);
            self.log.push(command);
            let Some(input) = input else { continue };
            let (next, commands) = workflow.transition(input);
            workflow = next;
            pending.extend(commands);
        }
    }

    fn answer(&mut self, command: &InstanceCommand) -> Option<InstanceInput> {
        match command {
            InstanceCommand::RunAction(action) => {
                let report = self.responder.respond(*action);
                self.blocked_attempt |= action.index() < 10 && report.outcome.failure().is_some();
                Some(InstanceInput::ActionDone(report))
            }
            InstanceCommand::RunCleanup(cleanup) => {
                self.finished.push(cleanup.action.index());
                Some(InstanceInput::CleanupDone)
            }
            InstanceCommand::Wait(_) => {
                self.retried_blocked |= self.blocked_attempt;
                self.blocked_attempt = false;
                Some(InstanceInput::RetryDue)
            }
            InstanceCommand::Abandon(_) | InstanceCommand::Finish(_) => None,
        }
    }
}

fn script() -> impl Strategy<Value = InstanceScript> {
    (0u32..3, 0u32..4, 0u32..3, prop::option::of(0u32..3)).prop_map(|(before, body, after, retry)| InstanceScript {
        before: (1..=before).map(sim::act).collect(),
        body: (10..10 + body).map(sim::act).collect(),
        after: (20..20 + after).map(sim::act).collect(),
        retry: retry.map(|attempts| Retry { attempts, delay: vc_model::Duration::new(Duration::from_millis(10)) }),
        allow_failure: None,
    })
}

/// Every cleanup the probe finished, then those abandoned, in order.
fn closed(probe: &Probe) -> Vec<u32> {
    let abandoned = probe.log.iter().filter_map(|command| match command {
        InstanceCommand::Abandon(cleanups) => Some(cleanups.iter().map(|a| a.index())),
        _ => None,
    });
    probe.finished.iter().copied().chain(abandoned.flatten()).collect()
}

proptest! {
    #[test]
    fn instance_cleanups_close_once_and_blocked_attempts_never_retry(
        script in script(),
        answers in answers(),
        interrupt in prop::option::of((0usize..30, prop::arbitrary::any::<bool>())),
    ) {
        let retry = script.retry.map_or(0, |r| r.attempts);
        let mut probe = Probe::new(answers);
        probe.run(InstanceWorkflow::new(Arc::new(script)), interrupt);
        let verdicts: Vec<_> = probe.log.iter().filter_map(|c| match c { InstanceCommand::Finish(v) => Some(v.clone()), _ => None }).collect();
        prop_assert_eq!(verdicts.len(), 1);
        let mut closed = closed(&probe);
        closed.sort_unstable();
        prop_assert_eq!(closed, probe.responder.registered.clone());
        prop_assert!(!probe.retried_blocked);
        prop_assert!(verdicts[0].attempts <= retry + 1);
        prop_assert!(interrupt.is_some() || verdicts[0].status != TestStatus::Skipped);
        let after_finish = probe.log.iter().skip_while(|c| !matches!(c, InstanceCommand::Finish(_))).count();
        prop_assert_eq!(after_finish, 1, "nothing after Finish");
    }

    #[test]
    fn a_blocked_instance_runs_nothing_and_is_not_retried(script in script()) {
        let mut probe = Probe::new(vec![Answer::Pass]);
        let (workflow, commands) = InstanceWorkflow::new(Arc::new(script)).transition(InstanceInput::Block(vc_model::Failure::new("x")));
        probe.log.extend(commands);
        probe.run(workflow, None);
        prop_assert!(matches!(&probe.log[..], [InstanceCommand::Finish(v)] if v.status == TestStatus::Blocked && v.attempts == 0));
    }
}

/// `count` `beforeAll` actions (IDs 1..), each needing the one before it when its flag is set.
fn before_all() -> impl Strategy<Value = Vec<bool>> {
    prop::collection::vec(prop::arbitrary::any::<bool>(), 0..4)
}

fn failed_shared(log: &[FileCommand], responder: &Responder) -> BTreeSet<ActionId> {
    let mut failed = responder.failed.clone();
    for command in log {
        if let FileCommand::Emit(Event::ActionFinished { action, outcome: ActionOutcome::Failed(_), .. }) = command {
            failed.insert(*action);
        }
    }
    failed
}

proptest! {
    #[test]
    fn files_launch_after_before_all_within_the_window_and_block_dependants(
        chain in before_all(),
        needs in prop::collection::vec(prop::option::of(1u32..5), 0..8),
        window in 1usize..4,
        answers in answers(),
    ) {
        let shared: Vec<_> = chain.iter().zip(1u32..).map(|(&linked, id)| {
            let needs = if linked && id > 1 { vec![id - 1] } else { Vec::new() };
            sim::planned(id, &needs)
        }).collect();
        let instances: Vec<_> = needs.iter().zip(0u32..).map(|(need, id)| sim::instance(id, need.as_slice())).collect();
        let file = sim::planned_file(shared, instances.clone(), &[9]);
        let mut responder = Responder::new(answers);
        let trace = sim::drive_file(FileOrchestrator::new(file, window), &mut |a| responder.respond(a), &mut |launch| {
            if launch.block.is_some() { TestStatus::Blocked } else { TestStatus::Passed }
        });
        let last_shared = trace.log.iter().rposition(|c| matches!(c, FileCommand::RunAction(a) if a.index() < 9));
        let first_launch = trace.log.iter().position(|c| matches!(c, FileCommand::Launch(_)));
        prop_assert!(first_launch.is_none() || last_shared < first_launch);
        prop_assert!(trace.max_running <= window);
        let failed = failed_shared(&trace.log, &responder);
        let launches: Vec<_> = trace.log.iter().filter_map(|c| match c { FileCommand::Launch(l) => Some(l), _ => None }).collect();
        prop_assert_eq!(launches.len(), instances.len());
        for launch in launches {
            prop_assert_eq!(launch.block.is_some(), launch.instance.needs.iter().any(|a| failed.contains(&a)));
        }
        let totals: Vec<_> = trace.log.iter().filter_map(|c| match c { FileCommand::Finish(end) => Some(end.totals), _ => None }).collect();
        prop_assert!(matches!(&totals[..], [t] if t.total() == instances.len()));
        let cleaned: Vec<_> = trace.log.iter().filter_map(|c| match c { FileCommand::RunCleanup(c) => Some(c.action.index()), _ => None }).rev().collect();
        prop_assert_eq!(cleaned, responder.registered);
    }

    #[test]
    fn files_start_only_after_before_run(count in 0u32..4, files in 0usize..3, answers in answers()) {
        let plan = Plan { before_run: (1..=count).map(|id| sim::planned(id, &[])).collect(), files: vec![(*sim::planned_file(vec![], vec![sim::instance(0, &[])], &[])).clone(); files], ..Plan::default() };
        let mut responder = Responder::new(answers);
        let log = sim::drive_run(RunOrchestrator::new(Arc::new(plan), false), &mut |a| responder.respond(a), &vec![Totals::default(); files]);
        let start = log.iter().position(|c| matches!(c, RunCommand::StartFiles(_)));
        let last_before = log.iter().rposition(|c| matches!(c, RunCommand::RunAction(_)));
        prop_assert!(files == 0 || last_before < start);
        let cleaned: Vec<_> = log.iter().filter_map(|c| match c { RunCommand::RunCleanup(c) => Some(c.action.index()), _ => None }).rev().collect();
        prop_assert_eq!(cleaned, responder.registered);
        prop_assert_eq!(log.iter().filter(|c| matches!(c, RunCommand::Finish(_))).count(), 1);
    }
}

fn instance_input() -> impl Strategy<Value = InstanceInput> {
    prop_oneof![
        Just(InstanceInput::Start),
        Just(InstanceInput::Block(vc_model::Failure::new("x"))),
        Just(InstanceInput::ActionDone(ActionReport::passed())),
        Just(InstanceInput::ActionDone(sim::fail(1))),
        Just(InstanceInput::ActionDone(sim::passing_with(&[100]))),
        Just(InstanceInput::ActionCut(Vec::new())),
        Just(InstanceInput::ActionCut(vec![PendingCleanup::new(sim::act(101))])),
        Just(InstanceInput::CleanupDone),
        Just(InstanceInput::RetryDue),
        Just(InstanceInput::Cancelled),
        Just(InstanceInput::Abort),
        Just(InstanceInput::ActionAborted(vec![PendingCleanup::new(sim::act(102))])),
    ]
}

fn file_input() -> impl Strategy<Value = FileInput> {
    prop_oneof![
        Just(FileInput::Start(super::FailedActions::new())),
        Just(FileInput::ActionDone(Box::new(ActionReport::passed()))),
        Just(FileInput::ActionDone(Box::new(sim::passing_with(&[100])))),
        Just(FileInput::ActionCut(Vec::new())),
        Just(FileInput::ActionCut(vec![PendingCleanup::new(sim::act(101))])),
        Just(FileInput::CleanupDone),
        Just(FileInput::CleanupAborted),
        Just(FileInput::InstanceDone(TestStatus::Passed)),
        Just(FileInput::Cancelled),
        Just(FileInput::Abort),
    ]
}

fn run_input() -> impl Strategy<Value = RunInput> {
    prop_oneof![
        Just(RunInput::Start),
        Just(RunInput::ActionDone(ActionReport::passed())),
        Just(RunInput::ActionDone(sim::passing_with(&[100]))),
        Just(RunInput::ActionCut(Vec::new())),
        Just(RunInput::ActionCut(vec![PendingCleanup::new(sim::act(101))])),
        Just(RunInput::CleanupDone),
        Just(RunInput::CleanupAborted),
        Just(RunInput::Tested(TestStatus::Failed)),
        Just(RunInput::FileDone(FileEnd::default())),
        Just(RunInput::Interrupt),
        Just(RunInput::Reported),
    ]
}

proptest! {
    #[test]
    fn any_input_sequence_finishes_an_instance_at_most_once(inputs in prop::collection::vec(instance_input(), 0..40)) {
        let script = InstanceScript { before: sim::acts(&[1]), body: sim::acts(&[10, 11]), after: sim::acts(&[20]), retry: Some(Retry { attempts: 2, delay: vc_model::Duration::default() }), allow_failure: None };
        let mut workflow = InstanceWorkflow::new(Arc::new(script));
        let mut finishes = 0;
        for input in inputs {
            let done = matches!(workflow.phase(), super::InstancePhase::Done(_));
            let (next, commands) = workflow.transition(input);
            prop_assert!(!done || commands.is_empty());
            finishes += commands.iter().filter(|c| matches!(c, InstanceCommand::Finish(_))).count();
            workflow = next;
        }
        prop_assert!(finishes <= 1);
    }

    #[test]
    fn any_input_sequence_finishes_a_file_at_most_once(inputs in prop::collection::vec(file_input(), 0..40)) {
        let file = sim::planned_file(vec![sim::planned(1, &[])], (0..3).map(|i| sim::instance(i, &[])).collect(), &[2]);
        let mut file = FileOrchestrator::new(file, 2);
        let mut finishes = 0;
        for input in inputs {
            let done = file.phase() == super::FilePhase::Done;
            let (next, commands) = file.transition(input);
            prop_assert!(!done || commands.is_empty());
            finishes += commands.iter().filter(|c| matches!(c, FileCommand::Finish(_))).count();
            file = next;
        }
        prop_assert!(finishes <= 1);
    }

    #[test]
    fn any_input_sequence_finishes_a_run_at_most_once(inputs in prop::collection::vec(run_input(), 0..40)) {
        let plan = Plan { before_run: vec![sim::planned(1, &[])], files: vec![(*sim::planned_file(vec![], vec![], &[])).clone(); 2], after_run: sim::acts(&[2]), ..Plan::default() };
        let mut run = RunOrchestrator::new(Arc::new(plan), true);
        let mut finishes = 0;
        for input in inputs {
            let done = matches!(run.phase(), super::RunPhase::Finished(_));
            let (next, commands) = run.transition(input);
            prop_assert!(!done || commands.is_empty());
            finishes += commands.iter().filter(|c| matches!(c, RunCommand::Finish(_))).count();
            run = next;
        }
        prop_assert!(finishes <= 1);
    }
}
