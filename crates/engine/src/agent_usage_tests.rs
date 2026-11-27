use vc_interfaces::{AgentAction, LoggedAction};
use vc_testkit::MemoryApprovals;

use super::{AgentTotals, DAY_MS, ToolUsage, UsageCount, Window, usage};
use crate::EngineError;
use crate::sample::agent_engine;

const FROM: u64 = 10 * DAY_MS;

fn at(day: u64, agent: &str, tool: &str, result: &str) -> LoggedAction {
    let action =
        AgentAction { agent: agent.into(), tool: tool.into(), result: result.into(), ..AgentAction::default() };
    LoggedAction { at_ms: FROM + day * DAY_MS + 1, action }
}

fn timed(mut logged: LoggedAction, took_ms: u64) -> LoggedAction {
    logged.action.took_ms = Some(took_ms);
    logged
}

fn log() -> Vec<LoggedAction> {
    let mut approved = timed(at(0, "claude", "call_api", "ok"), 100);
    approved.action.approved_by = Some("sam".into());
    let mut run = timed(at(0, "claude", "run_tests", "ok"), 2000);
    run.action.ran = true;
    let before = LoggedAction { at_ms: FROM - 1, ..at(0, "claude", "call_api", "ok") };
    vec![
        timed(at(2, "claude", "call_api", "failed: the api is gone"), 50),
        timed(at(1, "cursor", "read_report", "ok"), 5),
        at(1, "claude", "call_api", "refused: bare: agents.call is empty"),
        at(1, "claude", "call_api", "denied"),
        run,
        approved,
        before,
    ]
}

const fn count(calls: u64, runs: u64, took_ms: u64) -> UsageCount {
    UsageCount { calls, runs, took_ms }
}

#[test]
fn each_agent_is_counted_by_day_and_each_tool_by_how_its_calls_ended() {
    let counted = usage(&log(), &Window { from_ms: FROM, to_ms: FROM + 2 * DAY_MS + 5, days: 3 });
    let claude = AgentTotals {
        agent: "claude".into(),
        total: count(5, 1, 2150),
        daily: vec![count(2, 1, 2100), count(2, 0, 0), count(1, 0, 50)],
    };
    let cursor = AgentTotals {
        agent: "cursor".into(),
        total: count(1, 0, 5),
        daily: vec![count(0, 0, 0), count(1, 0, 5), count(0, 0, 0)],
    };
    assert_eq!(counted.agents, [claude, cursor], "the busiest first; a call before the window is left out");
    let tool = |agent: &str, tool: &str| ToolUsage { agent: agent.into(), tool: tool.into(), ..ToolUsage::default() };
    let calls =
        ToolUsage { calls: 4, ok: 1, failed: 1, refused: 2, approved: 1, took_ms: 150, ..tool("claude", "call_api") };
    let runs = ToolUsage { calls: 1, ok: 1, took_ms: 2000, ..tool("claude", "run_tests") };
    let reads = ToolUsage { calls: 1, ok: 1, took_ms: 5, ..tool("cursor", "read_report") };
    assert_eq!(counted.tools, [calls, runs, reads]);
    assert_eq!((counted.from_ms, counted.to_ms), (FROM, FROM + 2 * DAY_MS + 5));
}

#[test]
fn a_call_after_the_window_ends_is_left_out_and_one_at_its_end_is_counted() {
    let counted = usage(&log(), &Window { from_ms: FROM, to_ms: FROM + DAY_MS + 1, days: 2 });
    assert_eq!(counted.agents[0].total, count(4, 1, 2100), "{counted:?}");
    assert_eq!(counted.agents.len(), 2);
}

#[test]
fn the_window_is_seven_days_by_default_and_ninety_at_most() {
    let engine = agent_engine(&MemoryApprovals::new());
    let usage = engine.agent_usage(0).unwrap();
    assert_eq!((usage.from_ms, usage.to_ms, usage.agents.len()), (0, 0, 0), "the fixed clock is at 0");
    assert!(engine.agent_usage(90).is_ok());
    let refused = engine.agent_usage(91);
    let Err(EngineError::Selection { reason, .. }) = refused else { panic!("{refused:?}") };
    assert!(!reason.contains("91"), "the days asked for are not echoed: {reason}");
}
