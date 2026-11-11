//! The agent approval queue and the log of agent tool calls, both in `.vibranium/history.db`
//! (D195).
//!
//! Every value handed to these ports is already redacted: the engine runs arguments through
//! the redactor before it asks, so the store never sees a secret.

use std::fmt;
use std::time::Duration;

use async_trait::async_trait;
use vc_model::Method;

/// The approval queue an agent's gated call waits on, shared by every process on the project.
///
/// The store adapter implements it over `history.db`, so a stdio agent and a separate `vc ui`
/// or `vc approve` see the same queue; `vc-testkit` has a fake.
#[async_trait]
pub trait Approvals: Send + Sync {
    /// The requests, newest first; only the undecided ones when `pending_only`.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] when the queue cannot be read.
    fn list(&self, pending_only: bool) -> Result<Vec<Approval>, ApprovalError>;

    /// The request `id`, or `None` when the queue has none by that ID.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] when the queue cannot be read.
    fn get(&self, id: ApprovalId) -> Result<Option<Approval>, ApprovalError>;

    /// Adds `request` to the queue, undecided.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] when the queue cannot be written.
    fn create(&self, request: &ApprovalRequest) -> Result<ApprovalId, ApprovalError>;

    /// Records a person's decision. [`Verdict::AlwaysAllow`] also stores a standing rule for the
    /// request's profile, tool and each of its methods, which the result names.
    ///
    /// # Errors
    ///
    /// [`ApprovalError::Unknown`] for an ID not in the queue, [`ApprovalError::Decided`] for a
    /// request no longer pending.
    fn decide(&self, decision: &Decision) -> Result<Decided, ApprovalError>;

    /// Waits until the request is decided, at most `timeout`; it is then expired. The store wakes
    /// on changes to its file, so a decision from another process ends the wait at once.
    /// Dropping the future stops waiting and leaves the request pending.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] for an unknown ID or when the queue cannot be read.
    async fn wait(&self, id: ApprovalId, timeout: Duration) -> Result<ApprovalStatus, ApprovalError>;

    /// Whether a person chose "always allow" for this profile, tool and method.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] when the rules cannot be read.
    fn always_allowed(&self, rule: &StandingRule) -> Result<bool, ApprovalError>;
}

/// The log of every tool call an agent made, allowed or not (`agent_actions`).
pub trait AgentActionWriter: Send + Sync {
    /// Appends `action`.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] when the log cannot be written.
    fn record(&self, action: &AgentAction) -> Result<(), ApprovalError>;
}

/// A request's number in the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApprovalId(u64);

impl ApprovalId {
    /// The ID with this number.
    #[must_use]
    pub const fn new(number: u64) -> Self {
        Self(number)
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ApprovalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What an agent asks to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct ApprovalRequest {
    /// The agent's name, as its MCP client gave it.
    pub agent: String,
    /// The tool, such as `run_tests` or `call_api`.
    pub tool: String,
    /// The active profile.
    pub profile: String,
    /// The methods the call would send that need approval.
    pub methods: Vec<Method>,
    /// One line for a person: what would run or be sent.
    pub summary: String,
    /// The call's arguments as JSON text, redacted.
    pub arguments: String,
}

/// A request in the queue and where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Approval {
    /// Its number.
    pub id: ApprovalId,
    /// What was asked.
    pub request: ApprovalRequest,
    /// When it was asked, in Unix milliseconds.
    pub created_ms: u64,
    /// Where it stands.
    pub status: ApprovalStatus,
    /// Who decided it, when someone has.
    pub decided_by: Option<String>,
}

/// Where a request stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub enum ApprovalStatus {
    /// Not decided yet.
    Pending,
    /// Approved, once or always.
    Approved,
    /// Refused.
    Denied,
    /// No one decided in time.
    Expired,
}

/// A person's answer to a request.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Decision {
    /// The request.
    pub id: ApprovalId,
    /// The answer.
    pub verdict: Verdict,
    /// Who answered: the OS user for `vc approve`, the web UI's user otherwise.
    pub by: String,
}

/// What recording a decision did.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Decided {
    /// Where the request stands now.
    pub status: ApprovalStatus,
    /// The methods a standing rule was kept for: those of an always-allowed request, and none
    /// for any other verdict.
    pub kept: Vec<Method>,
}

/// The answer itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Verdict {
    /// Allow this once.
    Approve,
    /// Allow this, and from now on the same tool and methods under the same profile (D195).
    AlwaysAllow,
    /// Refuse.
    Deny,
}

/// A standing "always allow": a profile, a tool and one method.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct StandingRule {
    /// The profile.
    pub profile: String,
    /// The tool.
    pub tool: String,
    /// The method.
    pub method: Method,
}

/// One logged agent tool call.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct AgentAction {
    /// The agent's name.
    pub agent: String,
    /// The tool.
    pub tool: String,
    /// The active profile, when the tool has one.
    pub profile: Option<String>,
    /// The arguments as JSON text, redacted.
    pub arguments: String,
    /// What happened: `ok`, `refused: <rule>`, `denied`, `expired` or an error message.
    pub result: String,
    /// Who approved it, when it needed approval.
    pub approved_by: Option<String>,
}

/// Why the queue or the log could not be used.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ApprovalError {
    /// No request has this ID.
    #[error("no approval request {id}")]
    Unknown {
        /// The ID asked for.
        id: ApprovalId,
    },
    /// The request was already decided or expired.
    #[error("approval request {id} is no longer pending")]
    Decided {
        /// The request.
        id: ApprovalId,
    },
    /// The store failed.
    #[error("cannot use the approvals in {location}: {reason}")]
    Store {
        /// The database file.
        location: String,
        /// What went wrong.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{ApprovalError, ApprovalId};

    #[test]
    fn ids_show_their_number() {
        assert_eq!(ApprovalId::new(7).get(), 7);
        assert_eq!(ApprovalId::new(7).to_string(), "7");
    }

    #[test]
    fn errors_name_the_request() {
        assert_eq!(ApprovalError::Unknown { id: ApprovalId::new(3) }.to_string(), "no approval request 3");
        assert_eq!(
            ApprovalError::Decided { id: ApprovalId::new(3) }.to_string(),
            "approval request 3 is no longer pending"
        );
    }
}
