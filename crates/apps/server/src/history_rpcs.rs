//! The history and agent RPCs: `ListRuns`, `GetRun`, `Trends`, `ApiTrends`, `ListAgentActions`,
//! `ListApprovals` and `DecideApproval`, each over its engine method.

use std::sync::Arc;

use vc_contracts::engine::v1 as wire;
use vc_engine::{ApiTrend, Approval, ApprovalId, ApprovalStatus, LoggedAction, RunFilter, TestTrend, Verdict};

use crate::engine_api::{EngineApi, Rpcs};
use crate::wire_history::{run, test};
use crate::{Code, ConnectError};

/// The runs `Trends` and `ApiTrends` read when the request says 0: the middle of the Insights screen's
/// 10, 30 and 100.
const TREND_WINDOW: usize = 30;

/// Adds the history and agent RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("ListRuns", list_runs)
        .unary("GetRun", get_run)
        .unary("Trends", trends)
        .unary("ApiTrends", api_trends)
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
    let (most, filter) =
        (limit(request.limit), RunFilter { kind: request.kind, profile: request.profile, branch: request.branch });
    let report = api.blocking(move |engine| engine.history(most, &filter)).await?;
    Ok(wire::ListRunsResponse { runs: report.runs().iter().map(run).collect() })
}

async fn get_run(api: Arc<EngineApi>, request: wire::GetRunRequest) -> Result<wire::GetRunResponse, ConnectError> {
    let which = request.id.map_or_else(|| "last".to_owned(), |id| id.to_string());
    let stored = api.blocking(move |engine| engine.stored(&which)).await?;
    Ok(wire::GetRunResponse { run: Some(run(stored.run())), tests: stored.tests().iter().map(test).collect() })
}

/// `window` as the engine takes it: [`TREND_WINDOW`] for 0.
fn window(window: u32) -> usize {
    if window == 0 { TREND_WINDOW } else { limit(window) }
}

async fn trends(api: Arc<EngineApi>, request: wire::TrendsRequest) -> Result<wire::TrendsResponse, ConnectError> {
    let window = window(request.window);
    let filter = RunFilter { profile: request.profile, branch: request.branch, ..RunFilter::default() };
    let found = api.blocking(move |engine| engine.trends(window, &filter)).await?;
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

async fn api_trends(
    api: Arc<EngineApi>,
    request: wire::ApiTrendsRequest,
) -> Result<wire::ApiTrendsResponse, ConnectError> {
    let window = window(request.window);
    let filter = RunFilter { profile: request.profile, branch: request.branch, ..RunFilter::default() };
    let found = api.blocking(move |engine| engine.api_trends(window, &filter)).await?;
    Ok(wire::ApiTrendsResponse { trends: found.into_iter().map(api_trend).collect() })
}

fn api_trend(trend: ApiTrend) -> wire::ApiTrend {
    wire::ApiTrend {
        api_id: trend.api_id,
        method: trend.method,
        calls: trend.calls,
        p50_ms: trend.p50_ms,
        p95_ms: trend.p95_ms,
        statuses: trend.statuses.into_iter().map(u32::from).collect(),
    }
}

async fn list_agent_actions(
    api: Arc<EngineApi>,
    request: wire::ListAgentActionsRequest,
) -> Result<wire::ListAgentActionsResponse, ConnectError> {
    let limit = limit(request.limit);
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
    Ok(wire::DecideApprovalResponse { status: status(decided.status).into() })
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
