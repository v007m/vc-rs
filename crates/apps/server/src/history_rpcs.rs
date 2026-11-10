//! The history and agent RPCs: `ListRuns`, `GetRun`, `Trends`, `ListAgentActions`,
//! `ListApprovals` and `DecideApproval`, each over its engine method.

use std::sync::Arc;

use vc_contracts::engine::v1 as wire;
use vc_engine::{Approval, ApprovalId, ApprovalStatus, LoggedAction, TestTrend, Verdict};

use crate::engine_api::{EngineApi, Rpcs};
use crate::wire_history::{run, test};
use crate::{Code, ConnectError};

/// The runs `Trends` reads when the request says 0: the middle of the Insights screen's
/// 10, 30 and 100.
const TREND_WINDOW: usize = 30;

/// Adds the history and agent RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("ListRuns", list_runs)
        .unary("GetRun", get_run)
        .unary("Trends", trends)
        .unary("ListAgentActions", list_agent_actions)
        .unary("ListApprovals", list_approvals)
        .unary("DecideApproval", decide_approval)
}

/// `limit` as the engine takes it; 0, all, as the most a store can count.
fn limit(limit: u32) -> usize {
    match limit {
        0 => usize::try_from(i64::MAX).unwrap_or(usize::MAX),
        limit => usize::try_from(limit).unwrap_or(usize::MAX),
    }
}

async fn list_runs(
    api: Arc<EngineApi>,
    request: wire::ListRunsRequest,
) -> Result<wire::ListRunsResponse, ConnectError> {
    let limit = limit(request.limit);
    let report = api.blocking(move |engine| engine.history(limit)).await?;
    Ok(wire::ListRunsResponse { runs: report.runs().iter().map(run).collect() })
}

async fn get_run(api: Arc<EngineApi>, request: wire::GetRunRequest) -> Result<wire::GetRunResponse, ConnectError> {
    let which = request.id.map_or_else(|| "last".to_owned(), |id| id.to_string());
    let stored = api.blocking(move |engine| engine.stored(&which)).await?;
    Ok(wire::GetRunResponse { run: Some(run(stored.run())), tests: stored.tests().iter().map(test).collect() })
}

async fn trends(api: Arc<EngineApi>, request: wire::TrendsRequest) -> Result<wire::TrendsResponse, ConnectError> {
    let window = if request.window == 0 { TREND_WINDOW } else { limit(request.window) };
    let found = api.blocking(move |engine| engine.trends(window)).await?;
    Ok(wire::TrendsResponse { trends: found.into_iter().map(trend).collect() })
}

fn trend(trend: TestTrend) -> wire::TestTrend {
    wire::TestTrend {
        test_id: trend.test_id,
        results: trend.results,
        failed: trend.failed,
        flaky: trend.flaky,
        mean_ms: trend.mean_ms,
    }
}

async fn list_agent_actions(
    api: Arc<EngineApi>,
    request: wire::ListAgentActionsRequest,
) -> Result<wire::ListAgentActionsResponse, ConnectError> {
    let limit = usize::try_from(request.limit).unwrap_or(usize::MAX);
    let found = api.blocking(move |engine| engine.agent_actions(limit)).await?;
    Ok(wire::ListAgentActionsResponse { actions: found.into_iter().map(action).collect() })
}

fn action(logged: LoggedAction) -> wire::AgentAction {
    let action = logged.action;
    wire::AgentAction {
        agent: action.agent,
        tool: action.tool,
        profile: action.profile,
        arguments: action.arguments,
        result: action.result,
        approved_by: action.approved_by,
        at_ms: logged.at_ms,
    }
}

async fn list_approvals(
    api: Arc<EngineApi>,
    request: wire::ListApprovalsRequest,
) -> Result<wire::ListApprovalsResponse, ConnectError> {
    let found = api.blocking(move |engine| engine.approvals_list(request.pending_only)).await?;
    Ok(wire::ListApprovalsResponse { approvals: found.into_iter().map(approval).collect() })
}

fn approval(approval: Approval) -> wire::Approval {
    let request = approval.request;
    wire::Approval {
        id: approval.id.get(),
        agent: request.agent,
        tool: request.tool,
        profile: request.profile,
        methods: request.methods.into_iter().map(|method| method.name().to_owned()).collect(),
        summary: request.summary,
        arguments: request.arguments,
        created_ms: approval.created_ms,
        status: status(approval.status).into(),
        decided_by: approval.decided_by,
    }
}

async fn decide_approval(
    api: Arc<EngineApi>,
    request: wire::DecideApprovalRequest,
) -> Result<wire::DecideApprovalResponse, ConnectError> {
    let verdict = match request.verdict() {
        wire::Verdict::Approve => Verdict::Approve,
        wire::Verdict::AlwaysAllow => Verdict::AlwaysAllow,
        wire::Verdict::Deny => Verdict::Deny,
        wire::Verdict::Unspecified => {
            return Err(ConnectError::new(
                Code::InvalidArgument,
                "a decision needs a verdict: approve, always allow or deny",
            ));
        }
    };
    let id = ApprovalId::new(request.id);
    let decided = api.blocking(move |engine| engine.approvals_decide(id, verdict)).await?;
    Ok(wire::DecideApprovalResponse { status: status(decided).into() })
}

const fn status(status: ApprovalStatus) -> wire::ApprovalStatus {
    match status {
        ApprovalStatus::Pending => wire::ApprovalStatus::Pending,
        ApprovalStatus::Approved => wire::ApprovalStatus::Approved,
        ApprovalStatus::Denied => wire::ApprovalStatus::Denied,
        ApprovalStatus::Expired => wire::ApprovalStatus::Expired,
    }
}

#[cfg(test)]
#[path = "history_rpcs_tests.rs"]
mod tests;
