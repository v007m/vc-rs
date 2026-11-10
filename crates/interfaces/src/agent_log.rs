//! Reading the log of agent tool calls (`agent_actions`), for the web UI's agents page.

use crate::{AgentAction, AgentActionWriter, ApprovalError};

/// One logged tool call and when it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct LoggedAction {
    /// When it was recorded, in milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// The call.
    pub action: AgentAction,
}

/// Reads the log [`AgentActionWriter`] appends to.
pub trait AgentActionReader: Send + Sync {
    /// The last `limit` calls, newest first; all of them when `limit` is 0.
    ///
    /// # Errors
    ///
    /// An [`ApprovalError`] when the log cannot be read.
    fn recent(&self, limit: usize) -> Result<Vec<LoggedAction>, ApprovalError>;
}

/// The agent log, written and read through one store.
pub trait AgentActionLog: AgentActionWriter + AgentActionReader {}

impl<T: AgentActionWriter + AgentActionReader + ?Sized> AgentActionLog for T {}
