use std::time::Duration;

use serde_json::json;
use vc_interfaces::{ApprovalId, ApprovalStatus, Approvals, Decided, Verdict};
use vc_model::Method;
use vc_testkit::MemoryApprovals;

use super::{Admission, AgentOutcome, span};
use crate::EngineError;
use crate::policy::AgentCall;
use crate::sample::agent_engine;

fn delete(profile: &str) -> AgentCall {
    AgentCall {
        agent: "claude".into(),
        tool: "call_api".into(),
        profiles: vec![profile.into()],
        methods: vec![Method::Delete],
        summary: "DELETE users#remove".into(),
        arguments: json!({"api": "users#remove", "vars": {"password": "abc"}}),
        ..AgentCall::default()
    }
}

#[tokio::test]
async fn an_allowed_call_goes_ahead_without_asking() {
    let queue = MemoryApprovals::new();
    let admitted = agent_engine(&queue).admit(&delete("dev"), Duration::from_secs(1)).await;
    assert_eq!(admitted, Ok(Admission::Allowed { approved_by: None }));
    assert_eq!(queue.list(false).unwrap(), Vec::new());
}

#[tokio::test]
async fn a_refused_call_is_logged_and_never_queued() {
    let queue = MemoryApprovals::new();
    let admitted = agent_engine(&queue).admit(&delete("bare"), Duration::from_secs(1)).await.unwrap();
    let reason = "bare: agents.call and agents.approve leave out DELETE".to_owned();
    assert_eq!(admitted, Admission::Refused { reason: reason.clone() });
    assert_eq!(queue.list(false).unwrap(), Vec::new());
    let logged = queue.actions();
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0].result, format!("refused: {reason}"));
    assert_eq!(logged[0].profile.as_deref(), Some("bare"));
}

#[tokio::test]
async fn logged_calls_read_back_newest_first_with_their_arguments_redacted() {
    let queue = MemoryApprovals::new();
    let engine = agent_engine(&queue);
    let _ = engine.admit(&delete("bare"), Duration::from_secs(1)).await.unwrap();
    let _ = engine.admit(&delete("prod"), Duration::from_secs(1)).await.unwrap();
    let read = engine.agent_actions(0).unwrap();
    let profiles: Vec<_> = read.iter().map(|logged| logged.action.profile.as_deref()).collect();
    assert_eq!(profiles, [Some("prod"), Some("bare")]);
    assert!(read[0].at_ms > read[1].at_ms);
    assert!(read.iter().all(|logged| !logged.action.arguments.contains("abc")), "{read:?}");
    assert_eq!(engine.agent_actions(1).unwrap().len(), 1);
}

#[tokio::test]
async fn an_approved_call_goes_ahead_and_names_who_approved_it() {
    let queue = MemoryApprovals::answering(Verdict::Approve);
    let admitted = agent_engine(&queue).admit(&delete("staging"), Duration::from_secs(5)).await;
    assert_eq!(admitted, Ok(Admission::Allowed { approved_by: Some("tester".into()) }));
    let request = &queue.list(false).unwrap()[0].request;
    assert_eq!((request.profile.as_str(), request.methods.as_slice()), ("staging", [Method::Delete].as_slice()));
    assert!(!request.arguments.contains("abc"), "arguments are redacted: {}", request.arguments);
    assert!(request.arguments.contains("users#remove"), "{}", request.arguments);
}

#[tokio::test]
async fn a_denied_or_unanswered_call_is_refused() {
    let engine = agent_engine(&MemoryApprovals::answering(Verdict::Deny));
    let denied = engine.admit(&delete("staging"), Duration::from_secs(5)).await;
    assert_eq!(denied, Ok(Admission::Refused { reason: "request 1 was denied".into() }));
    let queue = MemoryApprovals::new();
    let unanswered = agent_engine(&queue).admit(&delete("staging"), Duration::from_millis(30)).await;
    assert_eq!(unanswered, Ok(Admission::Refused { reason: "no one approved request 1 within 0 seconds".into() }));
    assert_eq!(queue.list(false).unwrap()[0].status, ApprovalStatus::Expired);
}

#[tokio::test]
async fn a_finished_call_is_logged_redacted() {
    let queue = MemoryApprovals::new();
    let outcome = AgentOutcome { result: "ok".into(), approved_by: Some("sam".into()) };
    agent_engine(&queue).record_agent_action(&delete("dev"), &outcome).await.unwrap();
    let logged = queue.actions();
    assert_eq!((logged[0].result.as_str(), logged[0].approved_by.as_deref()), ("ok", Some("sam")));
    assert!(!logged[0].arguments.contains("abc"), "{}", logged[0].arguments);
}

#[tokio::test]
async fn the_queue_and_the_log_get_the_redact_rules_too() {
    let queue = MemoryApprovals::answering(Verdict::Deny);
    let call = AgentCall { arguments: json!({"note": "sent tok-4821"}), ..delete("staging") };
    let admitted = agent_engine(&queue).admit(&call, Duration::from_secs(5)).await;
    assert!(matches!(admitted, Ok(Admission::Refused { .. })), "{admitted:?}");
    let (asked, logged) = (queue.list(false).unwrap(), queue.actions());
    assert_eq!(asked[0].request.arguments, r#"{"note":"sent ••••"}"#);
    assert_eq!(logged[0].arguments, asked[0].request.arguments);
}

#[tokio::test]
async fn a_person_lists_and_decides_the_requests() {
    let queue = MemoryApprovals::new();
    let engine = agent_engine(&queue);
    let call = delete("staging");
    let waiting = engine.admit(&call, Duration::from_secs(5));
    let decide = async {
        while engine.approvals_list(true).unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        engine.approvals_decide(ApprovalId::new(1), Verdict::AlwaysAllow)
    };
    let (admitted, decided) = tokio::join!(waiting, decide);
    assert_eq!(decided, Ok(Decided { status: ApprovalStatus::Approved, kept: vec![Method::Delete] }));
    assert_eq!(admitted, Ok(Admission::Allowed { approved_by: Some("someone".into()) }));
    assert_eq!(engine.approvals_list(true), Ok(Vec::new()));
    let again = engine.admit(&delete("staging"), Duration::from_secs(5)).await;
    assert_eq!(again, Ok(Admission::Allowed { approved_by: None }), "always-allow lets it through");
}

#[test]
fn deciding_an_unknown_request_says_how_to_list_them() {
    let engine = agent_engine(&MemoryApprovals::new());
    let decided = engine.approvals_decide(ApprovalId::new(7), Verdict::Deny);
    let Err(EngineError::History { reason, help }) = decided else { panic!("{decided:?}") };
    assert_eq!(reason, "no approval request 7");
    assert_eq!(help.as_deref(), Some("`vc approvals` lists the waiting requests"));
}

#[test]
fn spans_read_as_minutes_when_whole() {
    assert_eq!(span(Duration::from_secs(600)), "10 minutes");
    assert_eq!(span(Duration::from_secs(60)), "1 minute");
    assert_eq!(span(Duration::from_secs(90)), "90 seconds");
}
