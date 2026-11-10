use std::sync::Arc;

use vc_model::{ActionId, Event, Failure, Instance, OwnerKey, PlannedFile, TestResult, TestStatus, Totals};

use super::instance::CANCELLED;
use super::{ActionReport, FailedActions, PendingCleanup, SharedActions};

/// Where a [`FileOrchestrator`] is; the index is the lifecycle action in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FilePhase {
    /// Waiting for `beforeRun` to finish.
    Waiting,
    /// Running `beforeAll[i]`.
    BeforeAll(usize),
    /// Instances are running.
    Running,
    /// Running `afterAll[i]`.
    AfterAll(usize),
    /// Running the file's cleanups, newest first.
    CleaningUp,
    /// Finished.
    Done,
}

/// What the file shell feeds a [`FileOrchestrator`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileInput {
    /// `beforeRun` finished; these run-level shared actions failed.
    Start(FailedActions),
    /// The lifecycle action in flight finished.
    ActionDone(ActionReport),
    /// The lifecycle action in flight was cut off by cancellation, with the cleanups its
    /// finished parts registered.
    ActionCut(Vec<PendingCleanup>),
    /// The cleanup in flight finished.
    CleanupDone,
    /// A launched instance finished.
    InstanceDone(TestStatus),
    /// The run was cancelled.
    Cancelled,
    /// The run was aborted.
    Abort,
}

/// An instance to start, blocked when a shared action it needs failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Launch {
    /// The instance.
    pub instance: Instance,
    /// The failure of the first shared action it needs that failed.
    pub block: Option<Failure>,
}

/// What a [`FileOrchestrator`] asks its shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileCommand {
    /// Run this lifecycle action and report [`FileInput::ActionDone`] or [`FileInput::ActionCut`].
    RunAction(ActionId),
    /// Run this cleanup and report [`FileInput::CleanupDone`].
    RunCleanup(PendingCleanup),
    /// Start this instance and report [`FileInput::InstanceDone`] when it ends.
    Launch(Launch),
    /// Publish this event.
    Emit(Event),
    /// The file is finished.
    Finish(Totals),
}

/// One test file: `beforeAll` first (D122), then its instances under a window, then `afterAll`
/// and its cleanups.
#[derive(Clone, Debug)]
pub struct FileOrchestrator {
    file: Arc<PlannedFile>,
    window: usize,
    phase: FilePhase,
    shared: SharedActions,
    next: usize,
    running: usize,
    cleanups: Vec<PendingCleanup>,
    totals: Totals,
    cancelled: bool,
    aborted: bool,
}

impl FileOrchestrator {
    /// A waiting file that keeps at most `window` instances running (at least one).
    #[must_use]
    pub fn new(file: Arc<PlannedFile>, window: usize) -> Self {
        let shared = SharedActions::new(OwnerKey::File(file.id));
        Self {
            file,
            window: window.max(1),
            phase: FilePhase::Waiting,
            shared,
            next: 0,
            running: 0,
            cleanups: Vec::new(),
            totals: Totals::default(),
            cancelled: false,
            aborted: false,
        }
    }

    /// Where the file is.
    #[must_use]
    pub const fn phase(&self) -> FilePhase {
        self.phase
    }

    /// The next state and the commands to carry out. Inputs that make no sense in the current
    /// state are ignored.
    #[must_use]
    pub fn transition(mut self, input: FileInput) -> (Self, Vec<FileCommand>) {
        let mut out = Vec::new();
        self.apply(input, &mut out);
        (self, out)
    }

    fn apply(&mut self, input: FileInput, out: &mut Vec<FileCommand>) {
        use FilePhase::{AfterAll, BeforeAll, CleaningUp, Done, Running, Waiting};
        match (self.phase, input) {
            (Done, _) => {}
            (Waiting, FileInput::Start(failed)) => self.start(failed, out),
            (BeforeAll(_) | AfterAll(_), FileInput::ActionDone(report)) => self.action_done(report, out),
            (BeforeAll(_) | AfterAll(_), FileInput::ActionCut(kept)) => {
                self.cleanups.extend(kept);
                self.wind_down(out);
            }
            (Running, FileInput::InstanceDone(status)) => self.instance_done(status, out),
            (CleaningUp, FileInput::CleanupDone) => self.next_cleanup(out),
            (_, FileInput::Cancelled) => self.cancel(out),
            (_, FileInput::Abort) => self.abort(out),
            _ => {}
        }
    }

    fn start(&mut self, failed: FailedActions, out: &mut Vec<FileCommand>) {
        self.shared.failed = failed;
        out.push(FileCommand::Emit(Event::FileStarted { file: self.file.id }));
        self.phase = FilePhase::BeforeAll(0);
        self.resume_before(out);
    }

    /// Starts the next `beforeAll` action; after the last, launches the instances.
    fn resume_before(&mut self, out: &mut Vec<FileCommand>) {
        let FilePhase::BeforeAll(index) = self.phase else { return };
        let mut events = Vec::new();
        let (index, next) = self.shared.next(&self.file.before_all, index, &mut events);
        out.extend(events.into_iter().map(FileCommand::Emit));
        self.phase = FilePhase::BeforeAll(index);
        if let Some(action) = next {
            return out.push(FileCommand::RunAction(action));
        }
        self.phase = FilePhase::Running;
        self.fill(out);
    }

    fn action_done(&mut self, report: ActionReport, out: &mut Vec<FileCommand>) {
        self.cleanups.extend(report.cleanups);
        match self.phase {
            FilePhase::BeforeAll(index) => {
                self.shared.record(self.file.before_all.get(index), report.outcome);
                self.phase = FilePhase::BeforeAll(index + 1);
                if self.cancelled { self.wind_down(out) } else { self.resume_before(out) }
            }
            FilePhase::AfterAll(index) => {
                self.phase = FilePhase::AfterAll(index + 1);
                self.resume_after(out);
            }
            _ => {}
        }
    }

    /// Launches instances up to the window; once all are done, moves on to `afterAll`.
    fn fill(&mut self, out: &mut Vec<FileCommand>) {
        while !self.cancelled && self.running < self.window {
            let Some(instance) = self.file.instances.get(self.next) else { break };
            let block = self.shared.blocking(&instance.needs);
            out.push(FileCommand::Launch(Launch { instance: instance.clone(), block }));
            self.next += 1;
            self.running += 1;
        }
        if self.running > 0 || (self.next < self.file.instances.len() && !self.cancelled) {
            return;
        }
        if self.aborted {
            self.finish(out);
        } else if self.cancelled {
            self.wind_down(out);
        } else {
            self.phase = FilePhase::AfterAll(0);
            self.resume_after(out);
        }
    }

    fn instance_done(&mut self, status: TestStatus, out: &mut Vec<FileCommand>) {
        self.totals.add(status);
        self.running = self.running.saturating_sub(1);
        self.fill(out);
    }

    fn resume_after(&mut self, out: &mut Vec<FileCommand>) {
        if let FilePhase::AfterAll(index) = self.phase
            && let Some(&action) = self.file.after_all.get(index)
            && !self.cancelled
        {
            return out.push(FileCommand::RunAction(action));
        }
        self.wind_down(out);
    }

    /// Reports the instances never launched as skipped and runs the cleanups.
    fn wind_down(&mut self, out: &mut Vec<FileCommand>) {
        self.skip_rest(out);
        self.phase = FilePhase::CleaningUp;
        self.next_cleanup(out);
    }

    fn skip_rest(&mut self, out: &mut Vec<FileCommand>) {
        for instance in self.file.instances.iter().skip(self.next) {
            self.totals.add(TestStatus::Skipped);
            out.push(FileCommand::Emit(Event::TestFinished { result: skipped(instance) }));
        }
        self.next = self.file.instances.len();
    }

    fn next_cleanup(&mut self, out: &mut Vec<FileCommand>) {
        match self.cleanups.pop() {
            Some(cleanup) => out.push(FileCommand::RunCleanup(cleanup)),
            None => self.finish(out),
        }
    }

    fn cancel(&mut self, out: &mut Vec<FileCommand>) {
        self.cancelled = true;
        match self.phase {
            FilePhase::Waiting => self.wind_down(out),
            FilePhase::Running => self.fill(out),
            _ => {}
        }
    }

    /// Stops now: abandons the cleanups, then finishes once running instances have stopped.
    fn abort(&mut self, out: &mut Vec<FileCommand>) {
        self.cancelled = true;
        self.aborted = true;
        if !self.cleanups.is_empty() {
            let cleanups = super::newest_first(&self.cleanups);
            out.push(FileCommand::Emit(Event::CleanupsAbandoned { owner: self.shared.owner, cleanups }));
            self.cleanups.clear();
        }
        self.skip_rest(out);
        if self.running == 0 {
            self.finish(out);
        } else {
            self.phase = FilePhase::Running;
        }
    }

    fn finish(&mut self, out: &mut Vec<FileCommand>) {
        self.phase = FilePhase::Done;
        out.push(FileCommand::Emit(Event::FileFinished { file: self.file.id, totals: self.totals }));
        out.push(FileCommand::Finish(self.totals));
    }
}

fn skipped(instance: &Instance) -> TestResult {
    TestResult {
        test: instance.test,
        instance: Some(instance.id),
        status: TestStatus::Skipped,
        attempts: 0,
        failure: None,
        skip_reason: Some(CANCELLED.to_owned()),
        duration: std::time::Duration::ZERO,
    }
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
