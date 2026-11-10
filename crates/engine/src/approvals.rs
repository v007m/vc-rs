//! The gate every agent call passes, and `vc approvals` and `vc approve <id>`: the queue of
//! agent calls waiting for a person (D195).
//!
//! [`Engine::admit`] asks the policy. An allowed call goes ahead, a refused one never starts,
//! and one that asks puts a request in the queue and waits for a person: approved, it goes
//! ahead; denied or not decided within the wait, it is refused. Arguments are redacted before
//! they reach the queue or the log.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use vc_adapter_store::SqliteApprovals;
use vc_adapter_system::SystemEnvironment;
use vc_executor::{MaskRules, Redactor};
use vc_interfaces::{
    AgentAction, AgentActionWriter, Approval, ApprovalError, ApprovalId, ApprovalRequest, ApprovalStatus, Approvals,
    Decision, Verdict,
};
use vc_model::Program;

use crate::policy::{AgentCall, Permission};
use crate::{Engine, EngineError};

/// How long an agent call waits for a person before it is refused (D195).
pub const APPROVAL_WAIT: Duration = Duration::from_secs(600);

/// What stands in for the arguments when the project does not compile, so nothing can be
/// redacted.
const WITHHELD: &str = "\"(withheld: the project does not compile)\"";

/// Whether an agent call may go ahead.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "closed set of gate answers, matched by the apps (D160)")]
pub enum Admission {
    /// The call goes ahead.
    Allowed {
        /// The person who approved it, when it asked.
        approved_by: Option<String>,
    },
    /// The call is refused; nothing is sent or written.
    Refused {
        /// Why, for the agent: the rule, the denial or the wait that ran out.
        reason: String,
    },
}

/// How an admitted agent call ended, for the log.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "built by the apps; every user is in this repo (D160)")]
pub struct AgentOutcome {
    /// One line: `ok`, `failed: …`.
    pub result: String,
    /// The person who approved the call, from [`Admission::Allowed`].
    pub approved_by: Option<String>,
}

impl Engine {
    /// Asks the policy whether `call` may go ahead and, when it asks, waits up to `wait` for a
    /// person to decide. A refusal is logged here; log an allowed call with
    /// [`Engine::record_agent_action`] once it ends.
    ///
    /// # Errors
    ///
    /// The errors of [`Engine::policy_check`], and [`EngineError::History`] when the queue
    /// or the log cannot be used.
    pub async fn admit(&self, call: &AgentCall, wait: Duration) -> Result<Admission, EngineError> {
        let (permission, program) = self.judged(call)?;
        let arguments = self.redacted(&program, &call.arguments).await;
        let admission = match permission {
            Permission::Allow => Admission::Allowed { approved_by: None },
            Permission::Refuse { rule } => Admission::Refused { reason: rule },
            Permission::Ask { profile, methods, code, .. } => {
                let (agent, tool, summary) = (call.agent.clone(), call.tool.clone(), with_code(&call.summary, &code));
                let request = ApprovalRequest { agent, tool, profile, methods, summary, arguments: arguments.clone() };
                self.ask(&request, wait).await?
            }
        };
        if let Admission::Refused { reason } = &admission {
            self.log(call, arguments, &AgentOutcome { result: format!("refused: {reason}"), approved_by: None })?;
        }
        Ok(admission)
    }

    /// Logs how an admitted agent call ended, with its arguments redacted.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the log cannot be written.
    pub async fn record_agent_action(&self, call: &AgentCall, outcome: &AgentOutcome) -> Result<(), EngineError> {
        let program = self.program().ok().map(|(program, _)| program);
        let arguments = match program {
            Some(program) => self.redacted(&program, &call.arguments).await,
            None => WITHHELD.to_owned(),
        };
        self.log(call, arguments, outcome)
    }

    /// The approval requests, newest first; only the pending ones when `pending_only`.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when the queue cannot be read.
    pub fn approvals_list(&self, pending_only: bool) -> Result<Vec<Approval>, EngineError> {
        self.approval_queue().list(pending_only).map_err(|error| store_error(&error))
    }

    /// Records the user's verdict on request `id`; `always-allow` also keeps a standing rule
    /// for its profiles, tool and methods.
    ///
    /// # Errors
    ///
    /// [`EngineError::History`] when there is no request `id`, it is decided already, or the
    /// queue cannot be written.
    pub fn approvals_decide(&self, id: ApprovalId, verdict: Verdict) -> Result<ApprovalStatus, EngineError> {
        let decision = Decision { id, verdict, by: self.person() };
        self.approval_queue().decide(&decision).map_err(|error| store_error(&error))
    }

    /// The queue the builder was given, else the one in the history under the root.
    pub(crate) fn approval_queue(&self) -> Arc<dyn Approvals> {
        match &self.approvals {
            Some((queue, _)) => Arc::clone(queue),
            None => Arc::new(SqliteApprovals::new(&self.history_path())),
        }
    }

    fn agent_log(&self) -> Arc<dyn AgentActionWriter> {
        match &self.approvals {
            Some((_, log)) => Arc::clone(log),
            None => Arc::new(SqliteApprovals::new(&self.history_path())),
        }
    }

    /// Queues `request` and waits for a person's verdict.
    async fn ask(&self, request: &ApprovalRequest, wait: Duration) -> Result<Admission, EngineError> {
        let queue = self.approval_queue();
        let id = queue.create(request).map_err(|error| store_error(&error))?;
        let status = queue.wait(id, wait).await.map_err(|error| store_error(&error))?;
        Ok(match status {
            ApprovalStatus::Approved => Admission::Allowed { approved_by: decided_by(queue.as_ref(), id) },
            ApprovalStatus::Denied => Admission::Refused { reason: format!("request {id} was denied") },
            ApprovalStatus::Pending | ApprovalStatus::Expired => {
                Admission::Refused { reason: format!("no one approved request {id} within {}", span(wait)) }
            }
        })
    }

    fn log(&self, call: &AgentCall, arguments: String, outcome: &AgentOutcome) -> Result<(), EngineError> {
        let action = AgentAction {
            agent: call.agent.clone(),
            tool: call.tool.clone(),
            profile: (!call.profiles.is_empty()).then(|| call.profiles.join(",")),
            arguments,
            result: outcome.result.clone(),
            approved_by: outcome.approved_by.clone(),
        };
        self.agent_log().record(&action).map_err(|error| store_error(&error))
    }

    /// `arguments` as JSON, masked as a run's events are: the config's built-in masking, then
    /// its `redact.rules` in order (a `function:` rule through the hook runner).
    async fn redacted(&self, program: &Program, arguments: &Value) -> String {
        let rules = MaskRules::standard().with_hooks(self.hook_runner(program), program).for_config(&program.config);
        Redactor::for_config(&program.config).with_rules(rules).masked(arguments.clone()).await.to_string()
    }

    /// Who is deciding: the OS user, as `USER` or `USERNAME` names them.
    fn person(&self) -> String {
        let environment = self.environment.clone().unwrap_or_else(|| Arc::new(SystemEnvironment::capture()));
        environment.var("USER").or_else(|| environment.var("USERNAME")).unwrap_or_else(|| "someone".to_owned())
    }
}

/// Who decided request `id`, when the queue still says.
fn decided_by(queue: &dyn Approvals, id: ApprovalId) -> Option<String> {
    queue.get(id).ok().flatten()?.decided_by
}

/// `10 minutes`, or `30 seconds` for a wait that is not whole minutes.
fn span(wait: Duration) -> String {
    let seconds = wait.as_secs();
    match (seconds / 60, seconds % 60) {
        (1, 0) => "1 minute".to_owned(),
        (minutes, 0) if minutes > 0 => format!("{minutes} minutes"),
        _ => format!("{seconds} seconds"),
    }
}

/// A queue or log failure as the engine's error.
pub fn store_error(error: &ApprovalError) -> EngineError {
    EngineError::History {
        reason: error.to_string(),
        help: Some("`vc approvals` lists the waiting requests".to_owned()),
    }
}

/// `summary`, then each `command:` and `function:` action the call would run (D202).
fn with_code(summary: &str, code: &[String]) -> String {
    if code.is_empty() { summary.to_owned() } else { format!("{summary}; runs {}", code.join("; ")) }
}

#[cfg(test)]
#[path = "approvals_tests.rs"]
mod tests;
