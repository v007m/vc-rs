use std::collections::VecDeque;
use std::sync::Arc;

use vc_model::{ActionId, OwnerKey, PlannedFile, TestStatus, Totals};

use super::crew::{Crew, Job};
use super::instance;
use super::instance::Ticket;
use super::tasks::Tasks;
use crate::cancel::CancelTree;
use crate::{FailedActions, FileCommand, FileInput, FileOrchestrator, Frame, Launch, SharedSlots};

/// One file to run: its plan, the `beforeRun` results and failures, and its frame.
pub struct FileJob {
    pub file: PlannedFile,
    pub failed: FailedActions,
    pub run: Arc<SharedSlots>,
    pub cancel: CancelTree,
}

/// Runs one file: this task owns its [`FileOrchestrator`] and carries out its commands, with a
/// worker per shared action, cleanup and instance.
pub async fn file(crew: Crew, job: FileJob) -> Totals {
    let slots = SharedSlots::new(job.file.before_all.iter().map(|planned| planned.action));
    let frame = Frame { run: job.run, file: Arc::new(slots), ..Frame::default() };
    let owner = OwnerKey::File(job.file.id);
    let mut machine = FileOrchestrator::new(Arc::new(job.file), crew.window);
    let mut shell =
        FileShell { crew, frame, owner, cancel: job.cancel, tasks: Tasks::new(), cancelled: false, aborted: false };
    let mut inputs = VecDeque::from([FileInput::Start(job.failed)]);
    loop {
        while let Some(input) = inputs.pop_front() {
            let commands;
            (machine, commands) = machine.transition(input);
            if let Some(totals) = commands.into_iter().find_map(|command| shell.carry_out(command)) {
                return totals;
            }
        }
        let Some(input) = shell.next_input().await else { return Totals::default() };
        inputs.push_back(input);
    }
}

struct FileShell {
    crew: Crew,
    frame: Frame,
    owner: OwnerKey,
    cancel: CancelTree,
    tasks: Tasks<FileInput>,
    cancelled: bool,
    aborted: bool,
}

impl FileShell {
    fn job(&self, action: ActionId) -> Job {
        Job { action, owner: self.owner, frame: self.frame.clone(), deadline: None }
    }

    fn carry_out(&mut self, command: FileCommand) -> Option<Totals> {
        match command {
            FileCommand::RunAction(action) => self.tasks.act(&self.crew, self.job(action), &self.cancel),
            FileCommand::RunCleanup(cleanup) => {
                self.tasks.clean(&self.crew, self.job(cleanup.action).cleaning(&cleanup.vars), &self.cancel);
            }
            FileCommand::Launch(launch) => self.launch(launch),
            FileCommand::Emit(event) => self.crew.bus.publish(event),
            FileCommand::Finish(totals) => return Some(totals),
        }
        None
    }

    fn launch(&mut self, launch: Launch) {
        let ticket = Ticket { launch, frame: self.frame.clone(), cancel: self.cancel.child() };
        let crew = self.crew.clone();
        let work = async move { FileInput::InstanceDone(instance::instance(crew, ticket).await) };
        self.tasks.spawn(work, FileInput::InstanceDone(TestStatus::Failed));
    }

    /// The next input: an abort or cancellation (each told once), else the next worker to end.
    /// `None` when nothing is running, which a well-formed machine never waits on.
    async fn next_input(&mut self) -> Option<FileInput> {
        if self.tasks.is_empty() {
            return None;
        }
        let input = tokio::select! {
            biased;
            () = self.cancel.aborted(), if !self.aborted => Some(FileInput::Abort),
            () = self.cancel.cancelled(), if !self.cancelled => Some(FileInput::Cancelled),
            input = self.tasks.next() => input,
        };
        self.aborted |= matches!(input, Some(FileInput::Abort));
        self.cancelled |= matches!(input, Some(FileInput::Abort | FileInput::Cancelled));
        input
    }
}
