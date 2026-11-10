use std::sync::Arc;

use vc_model::{ActionId, Event, OwnerKey, Plan, RunStatus, SkippedTest, TestResult, TestStatus, Totals};

use super::{ActionReport, FailedActions, PendingCleanup, SharedActions};

/// Where a [`RunOrchestrator`] is; the index is the lifecycle action in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunPhase {
    /// Not started.
    Planned,
    /// Running `beforeRun[i]`.
    BeforeRun(usize),
    /// Files are running.
    Running,
    /// Running `afterRun[i]`.
    AfterRun(usize),
    /// Running the run's cleanups, newest first.
    CleaningUp,
    /// Waiting for the events to reach the subscribers.
    Reporting,
    /// Finished.
    Finished(RunStatus),
}

/// What the run shell feeds a [`RunOrchestrator`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunInput {
    /// Start the run.
    Start,
    /// The lifecycle action in flight finished.
    ActionDone(ActionReport),
    /// The lifecycle action in flight was cut off by cancellation, with the cleanups its
    /// finished parts registered.
    ActionCut(Vec<PendingCleanup>),
    /// The cleanup in flight finished.
    CleanupDone,
    /// An instance ended with this status (for `--bail`).
    Tested(TestStatus),
    /// A file finished with these totals.
    FileDone(Totals),
    /// A Ctrl-C: the first cancels, the second skips the grace period, the third aborts.
    Interrupt,
    /// The events are with the subscribers.
    Reported,
}

/// How a run ended: the input to the exit code (D127).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunSummary {
    /// How it ended.
    pub status: RunStatus,
    /// Every instance and skipped test by status.
    pub totals: Totals,
}

/// What a [`RunOrchestrator`] asks its shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunCommand {
    /// Run this lifecycle action and report [`RunInput::ActionDone`] or [`RunInput::ActionCut`].
    RunAction(ActionId),
    /// Run this cleanup and report [`RunInput::CleanupDone`].
    RunCleanup(PendingCleanup),
    /// Start every file; these run-level shared actions failed. Report [`RunInput::FileDone`]
    /// for each.
    StartFiles(FailedActions),
    /// Publish this event.
    Emit(Event),
    /// Cancel the work: no new requests, a grace period for those in flight, then cleanups.
    Cancel,
    /// End the grace period now.
    SkipGrace,
    /// Stop everything now; cleanups that have not run are listed, not run.
    Abort,
    /// Make sure every event reached the subscribers, then report [`RunInput::Reported`].
    Report(RunSummary),
    /// The run is finished.
    Finish(RunSummary),
}

/// The whole run: `beforeRun` first (D122), then the files, `afterRun`, the run's cleanups and
/// the final status.
#[derive(Clone, Debug)]
pub struct RunOrchestrator {
    plan: Arc<Plan>,
    bail: bool,
    phase: RunPhase,
    shared: SharedActions,
    cleanups: Vec<PendingCleanup>,
    totals: Totals,
    files_left: usize,
    interrupts: u8,
    cancelled: bool,
    aborted: bool,
}

impl RunOrchestrator {
    /// A planned run; with `bail`, the first failed or blocked instance cancels the rest.
    #[must_use]
    pub fn new(plan: Arc<Plan>, bail: bool) -> Self {
        Self {
            plan,
            bail,
            phase: RunPhase::Planned,
            shared: SharedActions::new(OwnerKey::Run),
            cleanups: Vec::new(),
            totals: Totals::default(),
            files_left: 0,
            interrupts: 0,
            cancelled: false,
            aborted: false,
        }
    }

    /// Where the run is.
    #[must_use]
    pub const fn phase(&self) -> RunPhase {
        self.phase
    }

    /// The next state and the commands to carry out. Inputs that make no sense in the current
    /// state are ignored.
    #[must_use]
    pub fn transition(mut self, input: RunInput) -> (Self, Vec<RunCommand>) {
        let mut out = Vec::new();
        self.apply(input, &mut out);
        (self, out)
    }

    fn apply(&mut self, input: RunInput, out: &mut Vec<RunCommand>) {
        use RunPhase::{AfterRun, BeforeRun, CleaningUp, Finished, Planned, Reporting, Running};
        match (self.phase, input) {
            (Finished(_), _) => {}
            (Planned, RunInput::Start) => self.start(out),
            (BeforeRun(_) | AfterRun(_), RunInput::ActionDone(report)) => self.action_done(report, out),
            (BeforeRun(_) | AfterRun(_), RunInput::ActionCut(kept)) => self.action_cut(kept, out),
            (Running, RunInput::FileDone(totals)) => self.file_done(totals, out),
            (CleaningUp, RunInput::CleanupDone) => self.next_cleanup(out),
            (Reporting, RunInput::Reported) => self.finish(out),
            (_, RunInput::Tested(status)) => self.tested(status, out),
            (_, RunInput::Interrupt) => self.interrupt(out),
            _ => {}
        }
    }

    fn start(&mut self, out: &mut Vec<RunCommand>) {
        let (files, instances) = (self.plan.files.len(), self.plan.instance_count());
        out.push(RunCommand::Emit(Event::RunStarted { files, instances }));
        for test in &self.plan.skipped {
            self.totals.add(TestStatus::Skipped);
            out.push(RunCommand::Emit(Event::TestFinished { result: skipped(test) }));
        }
        self.phase = RunPhase::BeforeRun(0);
        self.resume_before(out);
    }

    /// Starts the next `beforeRun` action; after the last, or once cancelled, starts the files.
    fn resume_before(&mut self, out: &mut Vec<RunCommand>) {
        let RunPhase::BeforeRun(index) = self.phase else { return };
        let mut events = Vec::new();
        let (index, next) = self.shared.next(&self.plan.before_run, index, &mut events);
        out.extend(events.into_iter().map(RunCommand::Emit));
        self.phase = RunPhase::BeforeRun(index);
        match next.filter(|_| !self.cancelled) {
            Some(action) => out.push(RunCommand::RunAction(action)),
            None => self.start_files(out),
        }
    }

    fn action_done(&mut self, report: ActionReport, out: &mut Vec<RunCommand>) {
        self.cleanups.extend(report.cleanups);
        match self.phase {
            RunPhase::BeforeRun(index) => {
                self.shared.record(self.plan.before_run.get(index), report.outcome);
                self.phase = RunPhase::BeforeRun(index + 1);
                self.resume_before(out);
            }
            RunPhase::AfterRun(index) => {
                self.phase = RunPhase::AfterRun(index + 1);
                self.resume_after(out);
            }
            _ => {}
        }
    }

    /// A cut `beforeRun` action starts the files; a cut `afterRun` one, the cleanups.
    fn action_cut(&mut self, kept: Vec<PendingCleanup>, out: &mut Vec<RunCommand>) {
        self.cleanups.extend(kept);
        if matches!(self.phase, RunPhase::BeforeRun(_)) { self.start_files(out) } else { self.clean_up(out) }
    }

    /// Files start even after a cancel, so that each reports its instances as skipped.
    fn start_files(&mut self, out: &mut Vec<RunCommand>) {
        self.phase = RunPhase::Running;
        self.files_left = self.plan.files.len();
        if self.files_left == 0 {
            return self.after_files(out);
        }
        out.push(RunCommand::StartFiles(self.shared.failed.clone()));
    }

    fn file_done(&mut self, totals: Totals, out: &mut Vec<RunCommand>) {
        self.totals.merge(totals);
        self.files_left = self.files_left.saturating_sub(1);
        if self.files_left == 0 {
            self.after_files(out);
        }
    }

    fn after_files(&mut self, out: &mut Vec<RunCommand>) {
        if self.aborted {
            return self.finish_now(out);
        }
        self.phase = RunPhase::AfterRun(0);
        self.resume_after(out);
    }

    fn resume_after(&mut self, out: &mut Vec<RunCommand>) {
        if let RunPhase::AfterRun(index) = self.phase
            && let Some(&action) = self.plan.after_run.get(index)
            && !self.cancelled
        {
            return out.push(RunCommand::RunAction(action));
        }
        self.clean_up(out);
    }

    fn clean_up(&mut self, out: &mut Vec<RunCommand>) {
        self.phase = RunPhase::CleaningUp;
        self.next_cleanup(out);
    }

    fn next_cleanup(&mut self, out: &mut Vec<RunCommand>) {
        if let Some(cleanup) = self.cleanups.pop() {
            return out.push(RunCommand::RunCleanup(cleanup));
        }
        self.phase = RunPhase::Reporting;
        let summary = self.summary();
        out.push(RunCommand::Emit(Event::RunFinished { status: summary.status, totals: summary.totals }));
        out.push(RunCommand::Report(summary));
    }

    fn tested(&mut self, status: TestStatus, out: &mut Vec<RunCommand>) {
        if self.bail && matches!(status, TestStatus::Failed | TestStatus::Blocked) {
            self.cancel(out);
        }
    }

    fn interrupt(&mut self, out: &mut Vec<RunCommand>) {
        self.interrupts = self.interrupts.saturating_add(1);
        match self.interrupts {
            1 => self.cancel(out),
            2 => out.push(RunCommand::SkipGrace),
            _ => self.abort(out),
        }
    }

    fn cancel(&mut self, out: &mut Vec<RunCommand>) {
        if !self.cancelled {
            self.cancelled = true;
            out.push(RunCommand::Cancel);
        }
    }

    /// Stops now: abandons the run's cleanups, then finishes once the files have stopped.
    fn abort(&mut self, out: &mut Vec<RunCommand>) {
        if self.aborted {
            return;
        }
        self.cancelled = true;
        self.aborted = true;
        out.push(RunCommand::Abort);
        if !self.cleanups.is_empty() {
            let cleanups = super::newest_first(&self.cleanups);
            out.push(RunCommand::Emit(Event::CleanupsAbandoned { owner: OwnerKey::Run, cleanups }));
            self.cleanups.clear();
        }
        match self.phase {
            RunPhase::Running if self.files_left > 0 => {}
            RunPhase::Reporting => self.finish(out),
            _ => self.finish_now(out),
        }
    }

    /// Finishes without `afterRun`, cleanups or waiting for the subscribers.
    fn finish_now(&mut self, out: &mut Vec<RunCommand>) {
        let summary = self.summary();
        out.push(RunCommand::Emit(Event::RunFinished { status: summary.status, totals: summary.totals }));
        self.finish(out);
    }

    fn finish(&mut self, out: &mut Vec<RunCommand>) {
        let summary = self.summary();
        self.phase = RunPhase::Finished(summary.status);
        out.push(RunCommand::Finish(summary));
    }

    const fn summary(&self) -> RunSummary {
        let status = if self.interrupts > 0 {
            RunStatus::Cancelled
        } else if self.totals.failures() > 0 {
            RunStatus::Failed
        } else {
            RunStatus::Passed
        };
        RunSummary { status, totals: self.totals }
    }
}

fn skipped(test: &SkippedTest) -> TestResult {
    TestResult {
        test: test.test,
        instance: None,
        status: TestStatus::Skipped,
        attempts: 0,
        failure: None,
        skip_reason: Some(test.reason.clone()),
        duration: std::time::Duration::ZERO,
    }
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
