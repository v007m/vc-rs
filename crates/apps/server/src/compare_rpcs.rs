//! The Runs space's comparison RPCs: `CompareRuns` (D672) and `TestHistory` (D673), each over
//! its engine method.

use std::sync::Arc;

use vc_contracts::engine::v1 as wire;
use vc_contracts::results::v1 as results;
use vc_engine::{Retyped, RunComparison, SLOWER, ShapeChange, TestChange, TestResults, TestStatus};

use crate::engine_api::{EngineApi, Rpcs};
use crate::wire_history::{run, test};
use crate::{Code, ConnectError};

/// The test runs `TestHistory` reads when the request says 0.
const STRIP: usize = 20;

/// Adds the comparison RPCs.
pub fn register(rpcs: Rpcs) -> Rpcs {
    rpcs.unary("CompareRuns", compare_runs).unary("TestHistory", test_history)
}

async fn compare_runs(
    api: Arc<EngineApi>,
    request: wire::CompareRunsRequest,
) -> Result<wire::CompareRunsResponse, ConnectError> {
    let slower = factor(request.slower_threshold)?;
    let runs = (which(request.a), which(request.b));
    let comparison = api.blocking(move |engine| engine.compare_runs((&runs.0, &runs.1), slower)).await?;
    Ok(compared(comparison))
}

/// A share of the earlier time as the engine's factor: 1.2 for 0.2, [`SLOWER`] for 0.
fn factor(share: f64) -> Result<f64, ConnectError> {
    if !(share.is_finite() && share >= 0.0) {
        let message = "slower_threshold is a share of the earlier time of 0 or more: 0.2 for 20%";
        return Err(ConnectError::new(Code::InvalidArgument, message));
    }
    Ok(if share == 0.0 { SLOWER } else { 1.0 + share })
}

/// A run number as the engine takes it: `last` for 0.
fn which(run: u64) -> String {
    if run == 0 { "last".to_owned() } else { run.to_string() }
}

fn compared(comparison: RunComparison) -> wire::CompareRunsResponse {
    let changes = |changes: Vec<TestChange>| changes.into_iter().map(change).collect();
    wire::CompareRunsResponse {
        a: comparison.before,
        b: comparison.after,
        newly_failing: changes(comparison.newly_failing),
        newly_blocked: changes(comparison.newly_blocked),
        fixed: changes(comparison.fixed),
        still_failing: changes(comparison.still_failing),
        slower: changes(comparison.slower),
        faster: changes(comparison.faster),
        shapes: comparison.shapes.into_iter().map(shape).collect(),
    }
}

fn change(change: TestChange) -> wire::TestChange {
    wire::TestChange {
        test_id: change.test_id,
        label: change.label,
        before: status(change.before).into(),
        after: status(change.after).into(),
        before_ms: change.before_ms,
        after_ms: change.after_ms,
    }
}

fn status(status: Option<TestStatus>) -> results::TestStatus {
    status.map_or(results::TestStatus::Unspecified, results::TestStatus::from)
}

fn shape(change: ShapeChange) -> wire::ShapeChange {
    let retyped = |retyped: Retyped| wire::Retyped { path: retyped.path, before: retyped.before, after: retyped.after };
    wire::ShapeChange {
        api_id: change.api_id,
        status: u32::from(change.status),
        added: change.added,
        removed: change.removed,
        retyped: change.retyped.into_iter().map(retyped).collect(),
    }
}

async fn test_history(
    api: Arc<EngineApi>,
    request: wire::TestHistoryRequest,
) -> Result<wire::TestHistoryResponse, ConnectError> {
    let limit = if request.limit == 0 { STRIP } else { usize::try_from(request.limit).unwrap_or(usize::MAX) };
    let strip = api.blocking(move |engine| engine.test_history(&request.test, limit)).await?;
    Ok(wire::TestHistoryResponse { runs: strip.iter().map(result).collect() })
}

fn result(results: &TestResults) -> wire::TestRunResult {
    wire::TestRunResult {
        run: Some(run(&results.run)),
        status: status(results.status()).into(),
        duration_ms: results.duration_ms(),
        instances: results.instances.iter().map(test).collect(),
    }
}

#[cfg(test)]
#[path = "compare_rpcs_tests.rs"]
mod tests;
