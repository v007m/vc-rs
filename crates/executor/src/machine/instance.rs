use std::sync::Arc;
use std::time::Duration;

use vc_model::{ActionId, ActionOutcome, Failure, Retry, TestStatus};

use super::{ActionReport, PendingCleanup};

/// The reason a cancelled instance reports as skipped.
pub const CANCELLED: &str = "cancelled";

/// What one instance runs, flattened from its file and test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct InstanceScript {
    /// The file's `beforeEach`, then the test's `before`: a failure blocks the instance.
    pub before: Vec<ActionId>,
    /// The call or the steps: the first failure fails the instance and skips the rest.
    pub body: Vec<ActionId>,
    /// The test's `after`, then the file's `afterEach`: always run.
    pub after: Vec<ActionId>,
    /// `retry`: re-run the whole instance after a failure (D126).
    pub retry: Option<Retry>,
}

/// Where an [`InstanceWorkflow`] is; the index is the action in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstancePhase {
    /// Not started.
    Pending,
    /// Running `before[i]`.
    Before(usize),
    /// Running `body[i]`.
    Body(usize),
    /// Running `after[i]`.
    After(usize),
    /// Running the cleanups, newest first.
    Cleanup,
    /// Waiting for the `retry` delay.
    Waiting,
    /// Finished.
    Done(TestStatus),
}

/// What the instance shell feeds an [`InstanceWorkflow`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstanceInput {
    /// Shared lifecycle done and nothing it needs failed: start the first attempt.
    Start,
    /// A shared action it needs failed: it is blocked without running (D122).
    Block(Failure),
    /// The action in flight finished.
    ActionDone(ActionReport),
    /// The action in flight was cut off by cancellation, with no result but the cleanups its
    /// finished parts registered.
    ActionCut(Vec<PendingCleanup>),
    /// The cleanup in flight finished; its outcome changes no result.
    CleanupDone,
    /// The `retry` delay passed.
    RetryDue,
    /// The run was cancelled: no new action starts, cleanups still run.
    Cancelled,
    /// The run was aborted: stop now and list the cleanups that did not run.
    Abort,
}

/// What an [`InstanceWorkflow`] asks its shell to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstanceCommand {
    /// Run this action and report [`InstanceInput::ActionDone`] or [`InstanceInput::ActionCut`].
    RunAction(ActionId),
    /// Run this cleanup and report [`InstanceInput::CleanupDone`].
    RunCleanup(PendingCleanup),
    /// Wait for the `retry` delay and report [`InstanceInput::RetryDue`].
    Wait(Duration),
    /// These cleanups will not run (abort), newest first.
    Abandon(Vec<ActionId>),
    /// The instance is finished.
    Finish(Verdict),
}

/// How an instance ended.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Verdict {
    /// The status (D126).
    pub status: TestStatus,
    /// Attempts started.
    pub attempts: u32,
    /// The failure behind a failed or blocked status, or the failed attempt before a flaky pass.
    pub failure: Option<Failure>,
    /// Why it was skipped.
    pub skip_reason: Option<String>,
}

/// One test instance: `before`, body, `after`, cleanups, and `retry` of the whole (D126).
#[derive(Clone, Debug)]
pub struct InstanceWorkflow {
    script: Arc<InstanceScript>,
    phase: InstancePhase,
    cleanups: Vec<PendingCleanup>,
    attempt: u32,
    failure: Option<Failure>,
    earlier: Option<Failure>,
    blocked: bool,
    cancelled: bool,
    cut_short: bool,
}

impl InstanceWorkflow {
    /// A pending instance that will run `script`.
    #[must_use]
    pub const fn new(script: Arc<InstanceScript>) -> Self {
        Self {
            script,
            phase: InstancePhase::Pending,
            cleanups: Vec::new(),
            attempt: 0,
            failure: None,
            earlier: None,
            blocked: false,
            cancelled: false,
            cut_short: false,
        }
    }

    /// Where the instance is.
    #[must_use]
    pub const fn phase(&self) -> InstancePhase {
        self.phase
    }

    /// The next state and the commands to carry out. Inputs that make no sense in the current
    /// state are ignored.
    #[must_use]
    pub fn transition(mut self, input: InstanceInput) -> (Self, Vec<InstanceCommand>) {
        let mut out = Vec::new();
        self.apply(input, &mut out);
        (self, out)
    }

    fn apply(&mut self, input: InstanceInput, out: &mut Vec<InstanceCommand>) {
        use InstancePhase::{After, Before, Body, Cleanup, Done, Pending, Waiting};
        match (self.phase, input) {
            (Done(_), _) => {}
            (Pending, InstanceInput::Start) | (Waiting, InstanceInput::RetryDue) => self.begin_attempt(out),
            (Pending, InstanceInput::Block(failure)) => self.finish(TestStatus::Blocked, Some(failure), out),
            (Pending, InstanceInput::Cancelled | InstanceInput::Abort) => self.finish_cancelled(out),
            (Before(_) | Body(_) | After(_), InstanceInput::ActionDone(report)) => self.action_done(report, out),
            (Before(_) | Body(_) | After(_), InstanceInput::ActionCut(kept)) => {
                self.cleanups.extend(kept);
                self.cut(out);
            }
            (Cleanup, InstanceInput::CleanupDone) => self.next_cleanup(out),
            (Waiting, InstanceInput::Cancelled) => self.give_up_retrying(out),
            (_, InstanceInput::Cancelled) => self.cancelled = true,
            (_, InstanceInput::Abort) => self.abort(out),
            _ => {}
        }
    }

    fn begin_attempt(&mut self, out: &mut Vec<InstanceCommand>) {
        self.attempt += 1;
        self.failure = None;
        self.blocked = false;
        self.phase = InstancePhase::Before(0);
        self.resume(out);
    }

    fn action_done(&mut self, report: ActionReport, out: &mut Vec<InstanceCommand>) {
        self.cleanups.extend(report.cleanups);
        match report.outcome {
            ActionOutcome::Passed => self.phase = next_index(self.phase),
            ActionOutcome::Failed(failure) => self.record(failure),
        }
        if self.cancelled { self.cut(out) } else { self.resume(out) }
    }

    /// A failed `before` blocks and a failed body action fails, both skipping to `after`; a
    /// failed `after` action fails the instance unless something failed first.
    fn record(&mut self, failure: Failure) {
        let in_after = matches!(self.phase, InstancePhase::After(_));
        self.blocked |= matches!(self.phase, InstancePhase::Before(_));
        self.failure.get_or_insert(failure);
        self.phase = if in_after { next_index(self.phase) } else { InstancePhase::After(0) };
    }

    /// Starts the action at the cursor, moving to the next block when one is exhausted.
    fn resume(&mut self, out: &mut Vec<InstanceCommand>) {
        loop {
            let (actions, index, next) = match self.phase {
                InstancePhase::Before(i) => (&self.script.before[..], i, InstancePhase::Body(0)),
                InstancePhase::Body(i) => (&self.script.body[..], i, InstancePhase::After(0)),
                InstancePhase::After(i) => (&self.script.after[..], i, InstancePhase::Cleanup),
                _ => return self.next_cleanup(out),
            };
            if let Some(&action) = actions.get(index) {
                return out.push(InstanceCommand::RunAction(action));
            }
            self.phase = next;
        }
    }

    /// Cancellation stopped the attempt: skip what is left and run the cleanups.
    fn cut(&mut self, out: &mut Vec<InstanceCommand>) {
        self.cancelled = true;
        self.cut_short = true;
        self.phase = InstancePhase::Cleanup;
        self.next_cleanup(out);
    }

    fn next_cleanup(&mut self, out: &mut Vec<InstanceCommand>) {
        self.phase = InstancePhase::Cleanup;
        match self.cleanups.pop() {
            Some(cleanup) => out.push(InstanceCommand::RunCleanup(cleanup)),
            None => self.conclude(out),
        }
    }

    /// The attempt and its cleanups are done: retry it, or finish.
    fn conclude(&mut self, out: &mut Vec<InstanceCommand>) {
        if self.cut_short && self.failure.is_none() {
            self.failure = self.earlier.take();
        }
        let status = self.status();
        if status == TestStatus::Failed && self.retries_left() {
            self.earlier = self.failure.take();
            self.phase = InstancePhase::Waiting;
            let delay = self.script.retry.map(|retry| retry.delay.get()).unwrap_or_default();
            return out.push(InstanceCommand::Wait(delay));
        }
        let failure = self.failure.take().or_else(|| self.earlier.take());
        self.finish(status, failure, out);
    }

    const fn status(&self) -> TestStatus {
        if self.blocked {
            TestStatus::Blocked
        } else if self.failure.is_some() {
            TestStatus::Failed
        } else if self.cut_short {
            TestStatus::Skipped
        } else if self.earlier.is_some() {
            TestStatus::Flaky
        } else {
            TestStatus::Passed
        }
    }

    fn retries_left(&self) -> bool {
        !self.cancelled && self.script.retry.is_some_and(|retry| self.attempt <= retry.attempts)
    }

    /// Cancelled while waiting to retry: the failed attempt stands.
    fn give_up_retrying(&mut self, out: &mut Vec<InstanceCommand>) {
        self.cancelled = true;
        self.failure = self.earlier.take();
        self.conclude(out);
    }

    fn abort(&mut self, out: &mut Vec<InstanceCommand>) {
        self.cancelled = true;
        self.cut_short |= self.phase != InstancePhase::Cleanup;
        if !self.cleanups.is_empty() {
            out.push(InstanceCommand::Abandon(super::newest_first(&self.cleanups)));
            self.cleanups.clear();
        }
        self.conclude(out);
    }

    fn finish_cancelled(&mut self, out: &mut Vec<InstanceCommand>) {
        self.cut_short = true;
        self.finish(TestStatus::Skipped, None, out);
    }

    fn finish(&mut self, status: TestStatus, failure: Option<Failure>, out: &mut Vec<InstanceCommand>) {
        self.phase = InstancePhase::Done(status);
        let skip_reason = (status == TestStatus::Skipped).then(|| CANCELLED.to_owned());
        out.push(InstanceCommand::Finish(Verdict { status, attempts: self.attempt, failure, skip_reason }));
    }
}

const fn next_index(phase: InstancePhase) -> InstancePhase {
    match phase {
        InstancePhase::Before(i) => InstancePhase::Before(i + 1),
        InstancePhase::Body(i) => InstancePhase::Body(i + 1),
        InstancePhase::After(i) => InstancePhase::After(i + 1),
        other => other,
    }
}

#[cfg(test)]
#[path = "instance_tests.rs"]
mod tests;
