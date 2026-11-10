use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1::{RunStatus, TestStatus};
use vc_engine::{RunRequest, interrupts};

use super::{decide_approval, get_run, list_approvals, list_runs, trends};
use crate::Code;
use crate::engine_fixture::project;

#[tokio::test]
async fn an_empty_history_lists_no_run_and_has_no_last_one() {
    let (_dir, api) = project();
    let runs = list_runs(api.clone(), wire::ListRunsRequest::default()).await.unwrap();
    assert_eq!(runs.runs, []);
    let missing = get_run(api, wire::GetRunRequest::default()).await.unwrap_err();
    assert!(missing.message.contains("no run is stored yet"), "{missing:?}");
}

#[tokio::test]
async fn a_stored_run_comes_back_with_its_tests_and_trends() {
    let (_dir, api) = project();
    api.engine().run(&RunRequest::default(), interrupts().1).await.unwrap();
    let runs = list_runs(api.clone(), wire::ListRunsRequest { limit: 0 }).await.unwrap();
    let [listed] = runs.runs.as_slice() else { panic!("{runs:?}") };
    assert_eq!((listed.kind.as_str(), listed.status()), ("run", RunStatus::Failed));
    let found = get_run(api.clone(), wire::GetRunRequest { id: Some(listed.id) }).await.unwrap();
    assert_eq!(found.run.as_ref().map(|run| run.id), Some(listed.id));
    let [test] = found.tests.as_slice() else { panic!("{found:?}") };
    assert_eq!((test.label.as_str(), test.status()), ("users.crud#listsUsers", TestStatus::Failed));
    let found = trends(api, wire::TrendsRequest::default()).await.unwrap();
    let [trend] = found.trends.as_slice() else { panic!("{found:?}") };
    assert_eq!((trend.test_id.as_str(), trend.results, trend.failed), ("users.crud#listsUsers", 1, 1));
}

#[tokio::test]
async fn a_decision_needs_a_verdict_and_a_known_approval() {
    let (_dir, api) = project();
    let pending = list_approvals(api.clone(), wire::ListApprovalsRequest { pending_only: true }).await.unwrap();
    assert_eq!(pending.approvals, []);
    let none = decide_approval(api.clone(), wire::DecideApprovalRequest { id: 1, verdict: 0 }).await.unwrap_err();
    assert_eq!(none.code, Code::InvalidArgument);
    let unknown = wire::DecideApprovalRequest { id: 7, verdict: wire::Verdict::Approve.into() };
    assert_ne!(decide_approval(api, unknown).await.unwrap_err().code, Code::Internal);
}
