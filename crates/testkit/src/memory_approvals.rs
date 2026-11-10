use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::watch;
use vc_interfaces::{
    AgentAction, AgentActionReader, AgentActionWriter, Approval, ApprovalError, ApprovalId, ApprovalRequest,
    ApprovalStatus, Approvals, Decision, LoggedAction, StandingRule, Verdict,
};

/// An approval queue and agent call log held in memory.
///
/// The state sits in a `watch` channel, so a waiting call wakes as soon as a decision lands.
/// Clones share the queue: a test hands one to the engine and keeps another to decide or to read
/// the log. [`MemoryApprovals::answering`] decides every new request at once, as a person who is
/// always at the keyboard.
#[derive(Clone, Debug)]
pub struct MemoryApprovals {
    state: Arc<watch::Sender<State>>,
    answer: Option<Verdict>,
}

#[derive(Debug, Default)]
struct State {
    approvals: Vec<Approval>,
    rules: Vec<StandingRule>,
    actions: Vec<AgentAction>,
}

impl Default for MemoryApprovals {
    fn default() -> Self {
        Self { state: Arc::new(watch::Sender::new(State::default())), answer: None }
    }
}

impl MemoryApprovals {
    /// An empty queue in which requests wait until decided.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty queue that decides each new request with `verdict`, by `tester`.
    #[must_use]
    pub fn answering(verdict: Verdict) -> Self {
        Self { answer: Some(verdict), ..Self::default() }
    }

    /// Every logged agent call, oldest first.
    #[must_use]
    pub fn actions(&self) -> Vec<AgentAction> {
        self.state.borrow().actions.clone()
    }

    fn status(&self, id: ApprovalId) -> Result<ApprovalStatus, ApprovalError> {
        let state = self.state.borrow();
        state
            .approvals
            .iter()
            .find(|approval| approval.id == id)
            .map(|approval| approval.status)
            .ok_or(ApprovalError::Unknown { id })
    }

    /// Marks `id` expired when it is still pending, and says where it stands.
    fn expire(&self, id: ApprovalId) -> Result<ApprovalStatus, ApprovalError> {
        self.state.send_modify(|state| {
            let pending = state
                .approvals
                .iter_mut()
                .find(|approval| approval.id == id && approval.status == ApprovalStatus::Pending);
            if let Some(approval) = pending {
                approval.status = ApprovalStatus::Expired;
            }
        });
        self.status(id)
    }
}

/// What a verdict leaves a request as.
const fn settled(verdict: Verdict) -> ApprovalStatus {
    match verdict {
        Verdict::Approve | Verdict::AlwaysAllow => ApprovalStatus::Approved,
        Verdict::Deny => ApprovalStatus::Denied,
    }
}

impl State {
    fn decide(&mut self, decision: &Decision) -> Result<ApprovalStatus, ApprovalError> {
        let id = decision.id;
        let approval =
            self.approvals.iter_mut().find(|approval| approval.id == id).ok_or(ApprovalError::Unknown { id })?;
        if approval.status != ApprovalStatus::Pending {
            return Err(ApprovalError::Decided { id });
        }
        approval.status = settled(decision.verdict);
        approval.decided_by = Some(decision.by.clone());
        if decision.verdict == Verdict::AlwaysAllow {
            let request = &approval.request;
            let rules = request.methods.iter().map(|&method| StandingRule {
                profile: request.profile.clone(),
                tool: request.tool.clone(),
                method,
            });
            self.rules.extend(rules);
        }
        Ok(approval.status)
    }
}

#[async_trait]
impl Approvals for MemoryApprovals {
    fn list(&self, pending_only: bool) -> Result<Vec<Approval>, ApprovalError> {
        let state = self.state.borrow();
        let wanted = |approval: &&Approval| !pending_only || approval.status == ApprovalStatus::Pending;
        Ok(state.approvals.iter().rev().filter(wanted).cloned().collect())
    }

    fn get(&self, id: ApprovalId) -> Result<Option<Approval>, ApprovalError> {
        Ok(self.state.borrow().approvals.iter().find(|approval| approval.id == id).cloned())
    }

    fn create(&self, request: &ApprovalRequest) -> Result<ApprovalId, ApprovalError> {
        let mut id = ApprovalId::new(0);
        self.state.send_modify(|state| {
            id = ApprovalId::new(u64::try_from(state.approvals.len()).unwrap_or(u64::MAX).saturating_add(1));
            let status = ApprovalStatus::Pending;
            state.approvals.push(Approval { id, request: request.clone(), created_ms: 0, status, decided_by: None });
            if let Some(verdict) = self.answer {
                state.decide(&Decision { id, verdict, by: "tester".into() }).ok();
            }
        });
        Ok(id)
    }

    fn decide(&self, decision: &Decision) -> Result<ApprovalStatus, ApprovalError> {
        let mut decided = Err(ApprovalError::Unknown { id: decision.id });
        self.state.send_modify(|state| decided = state.decide(decision));
        decided
    }

    async fn wait(&self, id: ApprovalId, timeout: Duration) -> Result<ApprovalStatus, ApprovalError> {
        let mut changes = self.state.subscribe();
        let decided = async {
            loop {
                let status = self.status(id)?;
                if status != ApprovalStatus::Pending || changes.changed().await.is_err() {
                    return Ok(status);
                }
            }
        };
        tokio::time::timeout(timeout, decided).await.unwrap_or_else(|_| self.expire(id))
    }

    fn always_allowed(&self, rule: &StandingRule) -> Result<bool, ApprovalError> {
        Ok(self.state.borrow().rules.contains(rule))
    }
}

impl AgentActionWriter for MemoryApprovals {
    fn record(&self, action: &AgentAction) -> Result<(), ApprovalError> {
        self.state.send_modify(|state| state.actions.push(action.clone()));
        Ok(())
    }
}

/// Each call's `at_ms` is its number in the log, from 1, since the queue has no clock.
impl AgentActionReader for MemoryApprovals {
    fn recent(&self, limit: usize) -> Result<Vec<LoggedAction>, ApprovalError> {
        let mut logged: Vec<_> =
            (1_u64..).zip(self.actions()).map(|(at_ms, action)| LoggedAction { at_ms, action }).collect();
        logged.reverse();
        if limit > 0 {
            logged.truncate(limit);
        }
        Ok(logged)
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{ApprovalRequest, ApprovalStatus, Approvals, Verdict};

    use super::MemoryApprovals;
    use crate::{agent_action_reader_suite, agent_action_writer_suite, approvals_suite};

    #[tokio::test]
    async fn passes_the_approvals_suite() {
        approvals_suite(&mut || Box::new(MemoryApprovals::new()) as Box<dyn Approvals>).await;
    }

    #[test]
    fn passes_the_agent_action_reader_suite() {
        agent_action_reader_suite(&MemoryApprovals::new());
    }

    #[test]
    fn passes_the_agent_action_writer_suite() {
        let log = MemoryApprovals::new();
        agent_action_writer_suite(&log);
        assert_eq!(log.actions().len(), 2);
    }

    #[test]
    fn an_answering_queue_decides_each_request_at_once() {
        let request = ApprovalRequest {
            agent: "claude".into(),
            tool: "run_tests".into(),
            profile: "staging".into(),
            methods: Vec::new(),
            summary: "1 test".into(),
            arguments: "{}".into(),
        };
        let queue = MemoryApprovals::answering(Verdict::Deny);
        let id = queue.create(&request).unwrap();
        assert_eq!(queue.list(false).unwrap()[0].status, ApprovalStatus::Denied);
        assert_eq!(queue.list(false).unwrap()[0].id, id);
    }
}
