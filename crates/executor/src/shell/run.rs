use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::mpsc::UnboundedReceiver;
use vc_model::{ActionId, OwnerKey, Plan, Totals};

use super::crew::{Crew, Job};
use super::file::{FileJob, file};
use super::tasks::Tasks;
use crate::cancel::CancelTree;
use crate::{FailedActions, Frame, RunCommand, RunInput, RunOrchestrator, RunSummary, SharedSlots};

/// The run's own frame: its inbox (interrupts and test verdicts), its cancellation root and
/// whether the first failure stops it.
pub struct RunFrame {
    pub inbox: UnboundedReceiver<RunInput>,
    pub cancel: CancelTree,
    pub bail: bool,
}

/// Runs a plan: this task owns the [`RunOrchestrator`] and carries out its commands, with a
/// worker per shared action, cleanup and file.
pub async fn run(crew: Crew, plan: Arc<Plan>, frame: RunFrame) -> RunSummary {
    let slots = SharedSlots::new(plan.before_run.iter().map(|planned| planned.action));
    let frame_values = Frame { run: Arc::new(slots), ..Frame::default() };
    let mut machine = RunOrchestrator::new(Arc::clone(&plan), frame.bail);
    let mut shell =
        RunShell { crew, plan, frame: frame_values, cancel: frame.cancel, tasks: Tasks::new(), inbox: frame.inbox };
    let mut inputs = VecDeque::from([RunInput::Start]);
    loop {
        while let Some(input) = inputs.pop_front() {
            let commands;
            (machine, commands) = machine.transition(input);
            if let Some(summary) = commands.into_iter().find_map(|command| shell.carry_out(command)) {
                return summary;
            }
        }
        inputs.push_back(shell.next_input().await);
    }
}

struct RunShell {
    crew: Crew,
    plan: Arc<Plan>,
    frame: Frame,
    cancel: CancelTree,
    tasks: Tasks<RunInput>,
    inbox: UnboundedReceiver<RunInput>,
}

impl RunShell {
    fn job(&self, action: ActionId) -> Job {
        Job { action, owner: OwnerKey::Run, frame: self.frame.clone() }
    }

    fn carry_out(&mut self, command: RunCommand) -> Option<RunSummary> {
        match command {
            RunCommand::RunAction(action) => self.tasks.act(&self.crew, self.job(action), &self.cancel),
            RunCommand::RunCleanup(cleanup) => {
                self.tasks.clean(&self.crew, self.job(cleanup.action).cleaning(&cleanup.vars), &self.cancel);
            }
            RunCommand::StartFiles(failed) => self.start_files(&failed),
            RunCommand::Emit(event) => self.crew.bus.publish(event),
            RunCommand::Cancel => self.cancel.cancel(),
            RunCommand::SkipGrace => self.cancel.skip_grace(),
            RunCommand::Abort => {
                self.cancel.cancel();
                self.cancel.abort();
            }
            RunCommand::Report(_) => self.report(),
            RunCommand::Finish(summary) => return Some(summary),
        }
        None
    }

    fn start_files(&mut self, failed: &FailedActions) {
        for planned in &self.plan.files {
            let run = Arc::clone(&self.frame.run);
            let job = FileJob { file: planned.clone(), failed: failed.clone(), run, cancel: self.cancel.child() };
            let crew = self.crew.clone();
            self.tasks
                .spawn(async move { RunInput::FileDone(file(crew, job).await) }, RunInput::FileDone(Totals::default()));
        }
    }

    /// Lets every subscriber catch up before the run reports it finished.
    fn report(&mut self) {
        let bus = self.crew.bus.clone();
        self.tasks.spawn(
            async move {
                bus.flush().await;
                RunInput::Reported
            },
            RunInput::Reported,
        );
    }

    /// The next input: from the inbox (interrupts, verdicts) first, else the next worker to end.
    async fn next_input(&mut self) -> RunInput {
        loop {
            let input = tokio::select! {
                biased;
                input = self.inbox.recv() => input,
                input = self.tasks.next(), if !self.tasks.is_empty() => input,
            };
            if let Some(input) = input {
                return input;
            }
        }
    }
}
