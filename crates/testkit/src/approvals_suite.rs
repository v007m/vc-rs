use std::fmt::Debug;
use std::time::Duration;

use vc_interfaces::{
    AgentAction, AgentActionWriter, ApprovalError, ApprovalId, ApprovalRequest, ApprovalStatus, Approvals, Decision,
    StandingRule, Verdict,
};
use vc_model::Method;

/// Builds an empty approval queue.
pub type MakeApprovals<'a> = dyn FnMut() -> Box<dyn Approvals> + Send + 'a;

/// The contract every [`Approvals`] meets (D112, D195). `make` builds an empty queue.
pub async fn approvals_suite(make: &mut MakeApprovals<'_>) {
    queues_requests_newest_first(make().as_ref());
    decides_a_request_once(make().as_ref());
    keeps_always_allow_per_profile_tool_and_method(make().as_ref());
    waits_for_a_decision_or_expires(make().as_ref()).await;
}

/// The contract every [`AgentActionWriter`] meets (D112, D195).
///
/// # Panics
///
/// When the writer breaks the contract.
pub fn agent_action_writer_suite(writer: &dyn AgentActionWriter) {
    let action = AgentAction {
        agent: "claude".into(),
        tool: "call_api".into(),
        profile: Some("staging".into()),
        arguments: r#"{"api":"users#delete"}"#.into(),
        result: "ok".into(),
        approved_by: Some("sam".into()),
    };
    assert_eq!(writer.record(&action), Ok(()));
    let refused = AgentAction { profile: None, approved_by: None, result: "denied".into(), ..action };
    assert_eq!(writer.record(&refused), Ok(()));
}

/// What `result` holds, after asserting it is not an error.
fn must<T: Default + Debug>(result: Result<T, ApprovalError>) -> T {
    assert!(result.is_ok(), "{result:?}");
    result.unwrap_or_default()
}

/// A request by `claude` to send `methods` under `profile`, added to `queue`.
fn ask(queue: &dyn Approvals, profile: &str, methods: &[Method]) -> ApprovalId {
    let created = queue.create(&request(profile, methods));
    assert!(created.is_ok(), "{created:?}");
    created.unwrap_or(ApprovalId::new(0))
}

fn request(profile: &str, methods: &[Method]) -> ApprovalRequest {
    ApprovalRequest {
        agent: "claude".into(),
        tool: "call_api".into(),
        profile: profile.into(),
        methods: methods.to_vec(),
        summary: "DELETE /users/7".into(),
        arguments: r#"{"api":"users#delete"}"#.into(),
    }
}

fn decision(id: ApprovalId, verdict: Verdict) -> Decision {
    Decision { id, verdict, by: "sam".into() }
}

fn rule(profile: &str, tool: &str, method: Method) -> StandingRule {
    StandingRule { profile: profile.into(), tool: tool.into(), method }
}

fn queues_requests_newest_first(queue: &dyn Approvals) {
    assert_eq!(must(queue.list(false)), []);
    let first = ask(queue, "staging", &[Method::Delete]);
    let second = ask(queue, "prod", &[Method::Put]);
    assert_ne!(first, second);
    let listed = must(queue.list(true));
    assert_eq!(listed.iter().map(|approval| approval.id).collect::<Vec<_>>(), [second, first]);
    let oldest = listed.last().map(|approval| (&approval.request, approval.status, approval.decided_by.clone()));
    assert_eq!(oldest, Some((&request("staging", &[Method::Delete]), ApprovalStatus::Pending, None)));
}

fn decides_a_request_once(queue: &dyn Approvals) {
    let approved = ask(queue, "staging", &[Method::Delete]);
    let denied = ask(queue, "staging", &[Method::Delete]);
    assert_eq!(queue.decide(&decision(approved, Verdict::Approve)), Ok(ApprovalStatus::Approved));
    assert_eq!(queue.decide(&decision(denied, Verdict::Deny)), Ok(ApprovalStatus::Denied));
    assert_eq!(must(queue.list(true)), []);
    let all = must(queue.list(false));
    let statuses: Vec<_> = all.iter().map(|approval| (approval.status, approval.decided_by.as_deref())).collect();
    assert_eq!(statuses, [(ApprovalStatus::Denied, Some("sam")), (ApprovalStatus::Approved, Some("sam"))]);
    assert_eq!(queue.decide(&decision(approved, Verdict::Deny)), Err(ApprovalError::Decided { id: approved }));
    let unknown = ApprovalId::new(999);
    assert_eq!(queue.decide(&decision(unknown, Verdict::Approve)), Err(ApprovalError::Unknown { id: unknown }));
    let found = must(queue.get(approved)).map(|approval| (approval.id, approval.status, approval.decided_by));
    assert_eq!(found, Some((approved, ApprovalStatus::Approved, Some("sam".into()))));
    assert_eq!(queue.get(unknown), Ok(None));
    assert_eq!(queue.always_allowed(&rule("staging", "call_api", Method::Delete)), Ok(false), "once adds no rule");
}

fn keeps_always_allow_per_profile_tool_and_method(queue: &dyn Approvals) {
    assert_eq!(queue.always_allowed(&rule("staging", "call_api", Method::Put)), Ok(false));
    let id = ask(queue, "staging", &[Method::Put, Method::Delete]);
    assert_eq!(queue.decide(&decision(id, Verdict::AlwaysAllow)), Ok(ApprovalStatus::Approved));
    assert_eq!(queue.always_allowed(&rule("staging", "call_api", Method::Put)), Ok(true));
    assert_eq!(queue.always_allowed(&rule("staging", "call_api", Method::Delete)), Ok(true));
    assert_eq!(queue.always_allowed(&rule("staging", "call_api", Method::Patch)), Ok(false));
    assert_eq!(queue.always_allowed(&rule("prod", "call_api", Method::Put)), Ok(false));
    assert_eq!(queue.always_allowed(&rule("staging", "run_tests", Method::Put)), Ok(false));
}

async fn waits_for_a_decision_or_expires(queue: &dyn Approvals) {
    let decided = ask(queue, "staging", &[Method::Delete]);
    assert_eq!(queue.decide(&decision(decided, Verdict::Deny)), Ok(ApprovalStatus::Denied));
    assert_eq!(queue.wait(decided, Duration::from_secs(5)).await, Ok(ApprovalStatus::Denied));
    let waiting = ask(queue, "staging", &[Method::Delete]);
    assert_eq!(queue.wait(waiting, Duration::from_millis(50)).await, Ok(ApprovalStatus::Expired));
    assert!(must(queue.list(true)).is_empty(), "an expired request is no longer pending");
    assert_eq!(queue.decide(&decision(waiting, Verdict::Approve)), Err(ApprovalError::Decided { id: waiting }));
    let unknown = ApprovalId::new(999);
    assert_eq!(queue.wait(unknown, Duration::from_millis(50)).await, Err(ApprovalError::Unknown { id: unknown }));
}
