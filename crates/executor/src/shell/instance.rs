use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use vc_model::{ActionId, Event, OwnerKey, TestResult, TestStatus};

use super::crew::{Crew, Job};
use crate::cancel::{CancelTree, Ran};
use crate::{InstanceCommand, InstanceInput, InstanceWorkflow, Launch, RunInput, Scope, Verdict};

/// One instance to run: what the file launched, the shared values it reads and its frame.
pub struct Ticket {
    pub launch: Launch,
    pub scope: Scope,
    pub cancel: CancelTree,
}

/// Runs one test instance to its verdict. Its actions run one after another in this task,
/// which owns the instance's [`InstanceWorkflow`].
pub async fn instance(crew: Crew, ticket: Ticket) -> TestStatus {
    let _permit = Arc::clone(&crew.limit).acquire_owned().await.ok();
    let script = crew.scripts.get(&ticket.launch.instance.test).cloned().unwrap_or_default();
    let shell = InstanceShell { crew, ticket };
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
}

impl InstanceShell {
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
        Job { action, owner: self.owner(), scope: self.ticket.scope.clone() }
    }

    async fn carry_out(&self, command: InstanceCommand, inputs: &mut VecDeque<InstanceInput>) -> Option<TestStatus> {
        match command {
            InstanceCommand::RunAction(action) => inputs.extend(self.act(action).await),
            InstanceCommand::RunCleanup(cleanup) => inputs.push_back(self.clean(cleanup.action).await),
            InstanceCommand::Wait(delay) => inputs.push_back(self.wait(delay).await),
            InstanceCommand::Abandon(cleanups) => {
                self.crew.bus.publish(Event::CleanupsAbandoned { owner: self.owner(), cleanups });
            }
            InstanceCommand::Finish(verdict) => return Some(self.finish(verdict)),
        }
        None
    }

    /// Runs an action; once the frame is cancelled the workflow hears so before the result.
    async fn act(&self, action: ActionId) -> Vec<InstanceInput> {
        let ran = self.crew.act(self.job(action), &self.ticket.cancel).await;
        let cancelled = self.ticket.cancel.is_cancelled().then_some(InstanceInput::Cancelled);
        let result = match ran {
            Ran::Done(report) => InstanceInput::ActionDone(report),
            Ran::Cut => InstanceInput::ActionCut,
            Ran::Aborted => return vec![InstanceInput::Abort],
        };
        cancelled.into_iter().chain([result]).collect()
    }

    async fn clean(&self, action: ActionId) -> InstanceInput {
        match self.crew.clean(self.job(action), &self.ticket.cancel).await {
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

    fn finish(&self, verdict: Verdict) -> TestStatus {
        let Verdict { status, attempts, failure, skip_reason } = verdict;
        let instance = &self.ticket.launch.instance;
        let result =
            TestResult { test: instance.test, instance: Some(instance.id), status, attempts, failure, skip_reason };
        self.crew.bus.publish(Event::TestFinished { result });
        self.crew.run.send(RunInput::Tested(status)).ok();
        status
    }
}
