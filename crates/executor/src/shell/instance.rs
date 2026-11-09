use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use vc_model::{ActionId, Event, OwnerKey, TestResult, TestStatus};

use super::crew::{Crew, Deadline, Job};
use crate::cancel::{CancelTree, Ran};
use crate::{
    Frame, InstanceCommand, InstanceInput, InstanceScript, InstanceWorkflow, Launch, RunInput, Verdict, own_limit,
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
    let script = crew.scripts.get(&ticket.launch.instance.test).cloned().unwrap_or_default();
    let frame = Frame { instance: Some(Arc::new(ticket.launch.instance.clone())), ..ticket.frame.clone() };
    let mut shell = InstanceShell { crew, ticket, frame, script: Arc::clone(&script), deadline: None };
    shell.start_attempt();
    let mut workflow = InstanceWorkflow::new(script);
    let mut inputs = VecDeque::from([shell.first_input()]);
    while let Some(input) = inputs.pop_front() {
        let commands;
        (workflow, commands) = workflow.transition(input);
        for command in commands {
            if let Some(status) = shell.carry_out(command, &mut inputs).await {
                return status;
            }
        }
    }
    TestStatus::Failed
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
}

impl InstanceShell {
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

    /// The attempt's deadline, or for an `after` or `afterEach` action its own limit from now:
    /// they always run, so the attempt's passing deadline must not cut them.
    fn deadline(&self, action: ActionId) -> Option<Deadline> {
        if !self.script.after.contains(&action) {
            return self.deadline;
        }
        let cx = &self.crew.cx;
        cx.action(action).map(|found| Deadline::after(own_limit(&cx.program, found)))
    }

    async fn carry_out(
        &mut self,
        command: InstanceCommand,
        inputs: &mut VecDeque<InstanceInput>,
    ) -> Option<TestStatus> {
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
            InstanceCommand::Finish(verdict) => return Some(self.finish(verdict).await),
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
                self.frame.absorb(&self.crew.cx.program, action, &report);
                InstanceInput::ActionDone(report)
            }
            Ran::Cut => InstanceInput::ActionCut(kept),
            Ran::Aborted => return vec![InstanceInput::Abort],
        };
        cancelled.into_iter().chain([result]).collect()
    }

    async fn clean(&self, job: Job) -> InstanceInput {
        match self.crew.clean(job, &self.ticket.cancel).await {
            Ran::Aborted => InstanceInput::Abort,
            Ran::Done(_) | Ran::Cut => InstanceInput::CleanupDone,
        }
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
        let result =
            TestResult { test: instance.test, instance: Some(instance.id), status, attempts, failure, skip_reason };
        self.crew.bus.publish(Event::TestFinished { result }).await;
        self.crew.run.send(RunInput::Tested(status)).ok();
        status
    }
}
