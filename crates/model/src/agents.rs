//! A profile's `agents` rules: what an agent may do under it (D195).

use crate::Method;

/// `profiles[].agents`: what an agent (MCP or the web UI's agents page) may do under the profile.
///
/// The default is a profile with no `agents` block: reads only (list, search, get, check, query
/// history), no runs, no calls and no definition writes (D195).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct AgentPolicy {
    /// `run`: the agent may run tests, within `call` and `approve` for the methods they send.
    pub run: bool,
    /// `call`: the methods an agent's request may send without asking.
    pub call: Vec<Method>,
    /// `approve`: the methods an agent's request may send once a person approves it.
    pub approve: Vec<Method>,
    /// `writeDefinitions`.
    pub write_definitions: WriteDefinitions,
}

/// `writeDefinitions`: whether an agent may change definition files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum WriteDefinitions {
    /// `false`: never.
    #[default]
    Deny,
    /// `ask`: after a person approves the change, through the approval flow (D195).
    Ask,
    /// `true`: whenever the change compiles (D107).
    Allow,
}

#[cfg(test)]
mod tests {
    use super::{AgentPolicy, WriteDefinitions};

    #[test]
    fn no_agents_block_allows_only_reads() {
        let policy = AgentPolicy::default();
        assert!(!policy.run && policy.call.is_empty() && policy.approve.is_empty());
        assert_eq!(policy.write_definitions, WriteDefinitions::Deny);
    }
}
