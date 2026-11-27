//! The agents' usage (D709): the agent log over the last days in UTC, counted per agent with
//! a count for each day, and per agent and tool with how the calls ended. For the web UI's
//! agents page. The engine sees calls, runs and how long they took, not an agent's requests
//! or tokens, so those are not counted.

use std::collections::BTreeMap;

use vc_interfaces::{AgentAction, LoggedAction};

use crate::approvals::store_error;
use crate::{Engine, EngineError};

const DAY_MS: u64 = 86_400_000;

/// The days [`Engine::agent_usage`] covers when it is asked for 0.
pub const USAGE_DAYS: u32 = 7;

/// The most days [`Engine::agent_usage`] covers.
pub const MAX_USAGE_DAYS: u32 = 90;

/// What the agents did over a window of days.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AgentUsage {
    /// The start of the window's first day in UTC, in milliseconds since the Unix epoch.
    pub from_ms: u64,
    /// Now, when the window ends.
    pub to_ms: u64,
    /// Each agent that called a tool in the window, the busiest first.
    pub agents: Vec<AgentTotals>,
    /// Each agent and tool it called, by agent, then the most called first.
    pub tools: Vec<ToolUsage>,
}

/// How many calls, how many of them started a run, and how long the timed ones took.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct UsageCount {
    pub calls: u64,
    pub runs: u64,
    pub took_ms: u64,
}

/// One agent's calls in the window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AgentTotals {
    /// The agent, as its MCP client names itself.
    pub agent: String,
    pub total: UsageCount,
    /// One count for each day of the window, the oldest first.
    pub daily: Vec<UsageCount>,
}

/// One agent's calls to one tool in the window, by how they ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolUsage {
    pub agent: String,
    pub tool: String,
    pub calls: u64,
    /// Ended `ok`.
    pub ok: u64,
    /// Ended `failed`.
    pub failed: u64,
    /// Refused by the policy, denied by a person or not decided in time.
    pub refused: u64,
    /// Went ahead once a person approved them.
    pub approved: u64,
    pub took_ms: u64,
}

impl Engine {
    /// What the agents did over the last `days` days in UTC, today included; [`USAGE_DAYS`]
    /// when `days` is 0.
    ///
    /// # Errors
    ///
    /// [`EngineError::Selection`] when `days` is over [`MAX_USAGE_DAYS`];
    /// [`EngineError::History`] when the log cannot be read.
    pub fn agent_usage(&self, days: u32) -> Result<AgentUsage, EngineError> {
        let days = match days {
            0 => USAGE_DAYS,
            1..=MAX_USAGE_DAYS => days,
            _ => {
                let reason = format!("the agents' usage covers at most {MAX_USAGE_DAYS} days");
                return Err(EngineError::Selection { reason, help: Some("ask for fewer days, such as 30".to_owned()) });
            }
        };
        let to_ms = self.clock.now_ms();
        let from_ms = (to_ms / DAY_MS).saturating_sub(u64::from(days) - 1) * DAY_MS;
        let logged = self.agent_log().since(from_ms).map_err(|error| store_error(&error))?;
        Ok(usage(&logged, &Window { from_ms, to_ms, days }))
    }
}

/// The days counted: `days` of them from `from_ms`, the start of the first, to `to_ms`.
pub struct Window {
    pub from_ms: u64,
    pub to_ms: u64,
    pub days: u32,
}

/// `logged`, counted over `window`; a call outside it is left out.
pub fn usage(logged: &[LoggedAction], window: &Window) -> AgentUsage {
    let &Window { from_ms, to_ms, days } = window;
    let mut agents: BTreeMap<&str, AgentTotals> = BTreeMap::new();
    let mut tools: BTreeMap<(&str, &str), ToolUsage> = BTreeMap::new();
    for logged in logged.iter().filter(|logged| (from_ms..=to_ms).contains(&logged.at_ms)) {
        let action = &logged.action;
        let day = usize::try_from((logged.at_ms - from_ms) / DAY_MS).unwrap_or(usize::MAX);
        agents.entry(&action.agent).or_insert_with(|| AgentTotals::empty(&action.agent, days)).add(day, action);
        tools.entry((&action.agent, &action.tool)).or_insert_with(|| ToolUsage::empty(action)).add(action);
    }
    let mut agents: Vec<_> = agents.into_values().collect();
    agents.sort_by(|left, right| right.total.calls.cmp(&left.total.calls).then_with(|| left.agent.cmp(&right.agent)));
    let mut tools: Vec<_> = tools.into_values().collect();
    tools.sort_by(|left, right| {
        let busiest = right.calls.cmp(&left.calls).then_with(|| left.tool.cmp(&right.tool));
        left.agent.cmp(&right.agent).then(busiest)
    });
    AgentUsage { from_ms, to_ms, agents, tools }
}

impl UsageCount {
    fn add(&mut self, action: &AgentAction) {
        self.calls += 1;
        self.runs += u64::from(action.ran);
        self.took_ms += action.took_ms.unwrap_or(0);
    }
}

impl AgentTotals {
    fn empty(agent: &str, days: u32) -> Self {
        let daily = vec![UsageCount::default(); usize::try_from(days).unwrap_or(0)];
        Self { agent: agent.to_owned(), total: UsageCount::default(), daily }
    }

    fn add(&mut self, day: usize, action: &AgentAction) {
        self.total.add(action);
        if let Some(count) = self.daily.get_mut(day) {
            count.add(action);
        }
    }
}

impl ToolUsage {
    fn empty(action: &AgentAction) -> Self {
        Self { agent: action.agent.clone(), tool: action.tool.clone(), ..Self::default() }
    }

    fn add(&mut self, action: &AgentAction) {
        let result = action.result.as_str();
        let refused = ["refused", "denied", "expired"].iter().any(|word| result.starts_with(word));
        self.calls += 1;
        self.ok += u64::from(result == "ok");
        self.failed += u64::from(result.starts_with("failed"));
        self.refused += u64::from(refused);
        self.approved += u64::from(action.approved_by.is_some());
        self.took_ms += action.took_ms.unwrap_or(0);
    }
}

#[cfg(test)]
#[path = "agent_usage_tests.rs"]
mod tests;
