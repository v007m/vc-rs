use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use vc_model::{ActionId, Event, OwnerKey, TestResult, TestStatus};

use super::crew::{Crew, Deadline, Job};
use crate::cancel::{CancelTree, Ran};
use crate::{
    ActionReport, Frame, InstanceCommand, InstanceInput, InstanceScript, InstanceWorkflow, IterationCall,
    IterationReport, Launch, RunInput, Verdict, called,
};

/// One instance to run: what the file launched, the shared values it reads and its frame.
pub struct Ticket {
    pub launch: Launch,
    pub frame: Frame,
    pub cancel: CancelTree,
}

/// Runs one test instance to its verdict. Its actions run one after another in this task,
/// which owns the instance's [`InstanceWorkflow`] and its [`Frame`].
pub async fn instance(crew: Crew, ticket: Ticket) -> TestStatus {
    let _permit = Arc::clone(&crew.limit).acquire_owned().await.ok();
    let mut shell = InstanceShell::new(crew, ticket);
    match shell.drive().await {
        Some(verdict) => shell.finish(verdict).await,
        None => TestStatus::Failed,
    }
}

/// Runs one instance as a load iteration: as [`instance`] does, but with no limit on the
/// instances at once, and its verdict and requests come back as a report, not as an event.
pub async fn iteration(crew: Crew, ticket: Ticket) -> IterationReport {
    let mut shell = InstanceShell::new(crew, ticket);
    let verdict = shell.drive().await;
    let passed = verdict.as_ref().is_some_and(|verdict| verdict.status == TestStatus::Passed);
    let failure = verdict.and_then(|verdict| verdict.failure).map(|failure| failure.message);
    IterationReport { passed, calls: shell.calls, failure }
}

struct InstanceShell {
    crew: Crew,
    ticket: Ticket,
    /// What the instance's actions have left so far.
    frame: Frame,
    /// The instance's actions, to tell its `after` ones.
    script: Arc<InstanceScript>,
    /// When the current attempt must be done by, from the test's `timeout`.
    deadline: Option<Deadline>,
    /// Each request its actions and cleanups sent, in order.
    calls: Vec<IterationCall>,
    /// When the instance got its turn, which its result's duration runs from.
    started: Instant,
}

impl InstanceShell {
    fn new(crew: Crew, ticket: Ticket) -> Self {
        let script = crew.scripts.get(&ticket.launch.instance.test).cloned().unwrap_or_default();
        let frame = Frame { instance: Some(Arc::new(ticket.launch.instance.clone())), ..ticket.frame.clone() };
        let mut shell =
            Self { crew, ticket, frame, script, deadline: None, calls: Vec::new(), started: Instant::now() };
        shell.start_attempt();
        shell
    }

    /// Runs the instance's workflow to its verdict; `None` if it stops without one.
    async fn drive(&mut self) -> Option<Verdict> {
        let mut workflow = InstanceWorkflow::new(Arc::clone(&self.script));
        let mut inputs = VecDeque::from([self.first_input()]);
        while let Some(input) = inputs.pop_front() {
            let commands;
            (workflow, commands) = workflow.transition(input);
            for command in commands {
                if let Some(verdict) = self.carry_out(command, &mut inputs).await {
                    return Some(verdict);
                }
            }
        }
        None
    }

    /// Starts the attempt's clock: each attempt gets the test's whole `timeout`.
    fn start_attempt(&mut self) {
        let test = self.crew.cx.program.tests.get(&self.ticket.launch.instance.test);
        self.deadline = test.and_then(|test| test.timeout).map(|timeout| Deadline::attempt(timeout.get()));
    }

    fn first_input(&self) -> InstanceInput {
        match &self.ticket.launch.block {
            Some(failure) => InstanceInput::Block(failure.clone()),
            None if self.ticket.cancel.is_cancelled() => InstanceInput::Cancelled,
            None => InstanceInput::Start,
        }
    }

    const fn owner(&self) -> OwnerKey {
        OwnerKey::Instance(self.ticket.launch.instance.id)
    }

    fn job(&self, action: ActionId) -> Job {
        Job { action, owner: self.owner(), frame: self.frame.clone(), deadline: self.deadline(action) }
    }

    /// The attempt's deadline, or for an `after` or `afterEach` action only its own `timeout`
    /// from now (D192): they always run, so the attempt's passing deadline must not cut them,
    /// and without a `timeout` only their requests' own limits bound them.
    fn deadline(&self, action: ActionId) -> Option<Deadline> {
        if !self.script.after.contains(&action) {
            return self.deadline;
        }
        let timeout = self.crew.cx.action(action).and_then(|found| found.timeout)?;
        Some(Deadline::after(timeout.get()))
    }

    async fn carry_out(&mut self, command: InstanceCommand, inputs: &mut VecDeque<InstanceInput>) -> Option<Verdict> {
        match command {
            InstanceCommand::RunAction(action) => inputs.extend(self.act(action).await),
            InstanceCommand::RunCleanup(cleanup) => {
                inputs.push_back(self.clean(self.job(cleanup.action).cleaning(&cleanup.vars)).await);
            }
            InstanceCommand::Wait(delay) => {
                self.frame.restart();
                let due = self.wait(delay).await;
                self.start_attempt();
                inputs.push_back(due);
            }
            InstanceCommand::Abandon(cleanups) => {
                self.crew.bus.publish(Event::CleanupsAbandoned { owner: self.owner(), cleanups }).await;
            }
            InstanceCommand::Finish(verdict) => return Some(verdict),
        }
        None
    }

    /// Runs an action and takes in what it left; once the frame is cancelled the workflow hears
    /// so before the result.
    async fn act(&mut self, action: ActionId) -> Vec<InstanceInput> {
        let (ran, kept) = self.crew.act(self.job(action), &self.ticket.cancel).await;
        let cancelled = self.ticket.cancel.is_cancelled().then_some(InstanceInput::Cancelled);
        let result = match ran {
            Ran::Done(report) => {
                self.record(action, &report);
                self.frame.absorb(&self.crew.cx.program, action, &report);
                InstanceInput::ActionDone(report)
            }
            Ran::Cut => InstanceInput::ActionCut(kept),
            Ran::Aborted => return vec![InstanceInput::Abort],
        };
        cancelled.into_iter().chain([result]).collect()
    }

    async fn clean(&mut self, job: Job) -> InstanceInput {
        let action = job.action;
        match self.crew.clean(job, &self.ticket.cancel).await {
            Ran::Aborted => InstanceInput::Abort,
            Ran::Done(report) => {
                self.record(action, &report);
                InstanceInput::CleanupDone
            }
            Ran::Cut => InstanceInput::CleanupDone,
        }
    }

    /// Notes each request `action` sent, with the api it called.
    fn record(&mut self, action: ActionId, report: &ActionReport) {
        let api = self.crew.cx.action(action).and_then(|found| called(&self.crew.cx.program, found)).map(|api| api.id);
        let calls = report.exchanges.iter().map(|exchange| IterationCall {
            api,
            status: Some(exchange.response.status.as_u16()),
            elapsed: exchange.timing.total,
        });
        self.calls.extend(calls);
    }

    /// Waits out the retry delay, unless the frame is cancelled or aborted first.
    async fn wait(&self, delay: Duration) -> InstanceInput {
        let cancel = &self.ticket.cancel;
        tokio::select! {
            biased;
            () = cancel.aborted() => InstanceInput::Abort,
            () = cancel.cancelled() => InstanceInput::Cancelled,
            () = tokio::time::sleep(delay) => InstanceInput::RetryDue,
        }
    }

    async fn finish(&self, verdict: Verdict) -> TestStatus {
        let Verdict { status, attempts, failure, skip_reason } = verdict;
        let instance = &self.ticket.launch.instance;
        let (test, duration) = (instance.test, self.started.elapsed());
        let result = TestResult { test, instance: Some(instance.id), status, attempts, failure, skip_reason, duration };
        self.crew.bus.publish(Event::TestFinished { result }).await;
        self.crew.run.send(RunInput::Tested(status)).ok();
        status
    }
}
