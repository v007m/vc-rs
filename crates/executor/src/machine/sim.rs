//! Test drivers that play the shells' part: they answer each command at once, so a machine can be
//! run to the end from a script of action outcomes.

use std::collections::VecDeque;
use std::sync::Arc;

use vc_model::{
    ActionId, Failure, FileId, Instance, InstanceId, Needs, PlannedAction, PlannedFile, StringTable, TestId,
    TestStatus, Totals,
};

use super::{
    ActionReport, FileCommand, FileInput, FileOrchestrator, InstanceCommand, InstanceInput, InstanceWorkflow, Launch,
    PendingCleanup, RunCommand, RunInput, RunOrchestrator, Verdict,
};

/// Guards the drivers against a machine that never finishes.
const MAX_STEPS: usize = 10_000;

/// Answers an action with its report.
pub type Respond<'a> = &'a mut dyn FnMut(ActionId) -> ActionReport;

pub fn act(index: u32) -> ActionId {
    ActionId::new(index)
}

pub fn acts(indexes: &[u32]) -> Vec<ActionId> {
    indexes.iter().copied().map(act).collect()
}

pub fn fail(index: u32) -> ActionReport {
    ActionReport::failed(Failure::new(format!("action {index} failed")).at(act(index)))
}

/// A passed report that registers cleanups `cleanups`, oldest first.
pub fn passing_with(cleanups: &[u32]) -> ActionReport {
    let cleanups = cleanups.iter().map(|&n| PendingCleanup::new(act(n))).collect();
    ActionReport { cleanups, ..ActionReport::passed() }
}

pub fn planned(index: u32, needs: &[u32]) -> PlannedAction {
    PlannedAction { action: act(index), needs: acts(needs).into_iter().collect() }
}

pub fn file_and_test() -> (FileId, TestId) {
    let mut table = StringTable::new();
    let file = FileId::new(table.intern("users.crud").unwrap());
    (file, TestId::new(table.intern("users.crud#getsUser").unwrap()))
}

pub fn instance(index: u32, needs: &[u32]) -> Instance {
    let needs: Needs = acts(needs).into_iter().collect();
    Instance { id: InstanceId::new(index), test: file_and_test().1, example_row: None, repeat_index: 0, needs }
}

pub fn planned_file(before_all: Vec<PlannedAction>, instances: Vec<Instance>, after_all: &[u32]) -> Arc<PlannedFile> {
    Arc::new(PlannedFile {
        id: file_and_test().0,
        before_all,
        instances,
        after_all: acts(after_all),
        skipped: Vec::new(),
    })
}

/// Runs an instance from `first` to the end; returns every command in order.
pub fn drive_instance(workflow: InstanceWorkflow, first: InstanceInput, respond: Respond) -> Vec<InstanceCommand> {
    let (mut workflow, commands) = workflow.transition(first);
    let mut pending = VecDeque::from(commands);
    let mut log = Vec::new();
    for _ in 0..MAX_STEPS {
        let Some(command) = pending.pop_front() else { break };
        log.push(command.clone());
        let input = match command {
            InstanceCommand::RunAction(action) => InstanceInput::ActionDone(respond(action)),
            InstanceCommand::RunCleanup(_) => InstanceInput::CleanupDone,
            InstanceCommand::Wait(_) => InstanceInput::RetryDue,
            InstanceCommand::Abandon(_) | InstanceCommand::Finish(_) => continue,
        };
        let (next, commands) = workflow.transition(input);
        workflow = next;
        pending.extend(commands);
    }
    log
}

/// The actions run, in order.
pub fn ran(log: &[InstanceCommand]) -> Vec<u32> {
    log.iter()
        .filter_map(|command| match command {
            InstanceCommand::RunAction(action) => Some(action.index()),
            _ => None,
        })
        .collect()
}

/// The cleanups run, in order.
pub fn cleaned(log: &[InstanceCommand]) -> Vec<u32> {
    log.iter()
        .filter_map(|command| match command {
            InstanceCommand::RunCleanup(cleanup) => Some(cleanup.action.index()),
            _ => None,
        })
        .collect()
}

/// The verdict of the single `Finish` command.
pub fn verdict(log: &[InstanceCommand]) -> Verdict {
    let verdicts: Vec<_> = log
        .iter()
        .filter_map(|command| match command {
            InstanceCommand::Finish(verdict) => Some(verdict.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(verdicts.len(), 1, "exactly one Finish in {log:?}");
    verdicts[0].clone()
}

/// What a file driver saw.
#[derive(Debug, Default)]
pub struct FileTrace {
    /// Every command, in order.
    pub log: Vec<FileCommand>,
    /// The most instances running at once.
    pub max_running: usize,
}

/// Runs a file from `Start(failed)`: lifecycle actions are answered by `respond`, and launched
/// instances finish one at a time, oldest first, with the status `outcome` gives them.
pub fn drive_file(
    file: FileOrchestrator,
    respond: Respond,
    outcome: &mut dyn FnMut(&Launch) -> TestStatus,
) -> FileTrace {
    let (mut file, commands) = file.transition(FileInput::Start(super::FailedActions::new()));
    let mut pending = VecDeque::from(commands);
    let (mut running, mut trace) = (VecDeque::new(), FileTrace::default());
    for _ in 0..MAX_STEPS {
        let input = if let Some(command) = pending.pop_front() {
            let input = file_answer(&command, respond, &mut running);
            trace.log.push(command);
            trace.max_running = trace.max_running.max(running.len());
            input
        } else if let Some(launch) = running.pop_front() {
            Some(FileInput::InstanceDone(outcome(&launch)))
        } else {
            break;
        };
        let Some(input) = input else { continue };
        let (next, commands) = file.transition(input);
        file = next;
        pending.extend(commands);
    }
    trace
}

fn file_answer(command: &FileCommand, respond: Respond, running: &mut VecDeque<Launch>) -> Option<FileInput> {
    match command {
        FileCommand::RunAction(action) => Some(FileInput::ActionDone(Box::new(respond(*action)))),
        FileCommand::RunCleanup(_) => Some(FileInput::CleanupDone),
        FileCommand::Launch(launch) => {
            running.push_back(launch.clone());
            None
        }
        FileCommand::Emit(_) | FileCommand::Finish(_) => None,
    }
}

/// Runs a run from `Start`: lifecycle actions are answered by `respond`, the files finish with
/// `files` (one entry per file), and reporting is acknowledged at once.
pub fn drive_run(run: RunOrchestrator, respond: Respond, files: &[Totals]) -> Vec<RunCommand> {
    let (mut run, commands) = run.transition(RunInput::Start);
    let mut pending = VecDeque::from(commands);
    let mut log = Vec::new();
    for _ in 0..MAX_STEPS {
        let Some(command) = pending.pop_front() else { break };
        log.push(command.clone());
        let inputs = match command {
            RunCommand::RunAction(action) => vec![RunInput::ActionDone(respond(action))],
            RunCommand::RunCleanup(_) => vec![RunInput::CleanupDone],
            RunCommand::StartFiles(_) => files.iter().map(|&totals| RunInput::FileDone(totals.into())).collect(),
            RunCommand::Report(_) => vec![RunInput::Reported],
            _ => continue,
        };
        for input in inputs {
            let (next, commands) = run.transition(input);
            run = next;
            pending.extend(commands);
        }
    }
    log
}
