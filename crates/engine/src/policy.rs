//! The agent policy (D195): whether an agent's tool call may go ahead, is refused, or waits
//! for a person's approval.
//!
//! Each field of a call is judged on its own under every active profile, and the most
//! restrictive answer wins: `run` for a call that runs tests, each method it sends against
//! `call` (allowed) and `approve` (asks), and `writeDefinitions` for a call that writes. A
//! profile without an `agents` block allows reads only. A person's "always allow" for the
//! profiles, tool and method turns a method that asks into one that is allowed.

use std::sync::Arc;

use serde_json::Value;
use vc_interfaces::{ConfigEntry, StandingRule};
use vc_model::{AgentPolicy, Method, ProfileId, Program, WriteDefinitions};

use crate::approvals::store_error;
use crate::plan::{self, profile_flag};
use crate::{Engine, EngineError};

/// The label the policy uses when the project has no profile.
const NO_PROFILE: &str = "no profile";

/// One tool call an agent wants to make, as the policy sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps with `..Default::default()`; every user is in this repo (D160)"
)]
pub struct AgentCall {
    /// The agent's name, from the MCP client.
    pub agent: String,
    /// The tool: `run_tests`, `call_api`, `write_definition`.
    pub tool: String,
    /// `--profile` as the agent gave it, in order; the default profiles when empty.
    pub profiles: Vec<String>,
    /// The HTTP methods the call would send.
    pub methods: Vec<Method>,
    /// Whether the call runs tests (`agents.run`).
    pub runs: bool,
    /// Whether the call writes definition files (`agents.writeDefinitions`).
    pub writes: bool,
    /// One line for a person: what would run, be sent or be written.
    pub summary: String,
    /// The call's arguments, as the agent sent them; redacted before they are stored.
    pub arguments: Value,
}

/// What the policy says about an [`AgentCall`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "closed set of policy answers, matched by the apps (D160)")]
pub enum Permission {
    /// The call goes ahead.
    Allow,
    /// The call is refused; nothing is sent or written.
    Refuse {
        /// The profile's `agents` rule that refuses it, such as `prod: agents.run is false`.
        rule: String,
    },
    /// The call waits for a person to approve it (`vc approve <id>`).
    Ask {
        /// The active profiles, comma-joined: what an approval and an always-allow are for.
        profile: String,
        /// The methods that need approval.
        methods: Vec<Method>,
        /// Whether the write needs approval (`writeDefinitions: ask`).
        writes: bool,
    },
}

/// The active profiles of a call: their IDs in the program, and their names.
pub struct Scope<'a> {
    pub program: &'a Program,
    pub ids: &'a [ProfileId],
    pub names: &'a [String],
}

impl Engine {
    /// What the `agents` blocks of the active profiles say about `call`; the most restrictive
    /// rule wins for each field, and a person's always-allow rules apply (D195).
    ///
    /// # Errors
    ///
    /// The errors of loading the project and choosing its profiles, and
    /// [`EngineError::History`] when the always-allow rules cannot be read.
    pub fn policy_check(&self, call: &AgentCall) -> Result<Permission, EngineError> {
        self.judged(call).map(|(permission, _)| permission)
    }

    /// [`Engine::policy_check`], with the program it judged under: its config says what to
    /// redact from the call's arguments.
    pub(crate) fn judged(&self, call: &AgentCall) -> Result<(Permission, Arc<Program>), EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&call.profiles).into_iter().collect();
        let layered = self.layered(&[], &flags)?;
        let (ids, names) = plan::profiles(&layered.program, layered.settings.profiles.as_ref())?;
        let permission = self.permission(&Scope { program: &layered.program, ids: &ids, names: &names }, call)?;
        Ok((permission, layered.program))
    }

    /// The policy's answer for `call` under `active`, with the always-allow rules applied.
    pub(crate) fn permission(&self, active: &Scope<'_>, call: &AgentCall) -> Result<Permission, EngineError> {
        let found = active.ids.iter().zip(active.names);
        let mut policies: Vec<(&str, &AgentPolicy)> = found
            .filter_map(|(id, name)| active.program.profiles.get(id).map(|profile| (name.as_str(), &profile.agents)))
            .collect();
        let none = AgentPolicy::default();
        if policies.is_empty() {
            policies.push((NO_PROFILE, &none));
        }
        let profile = policies.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(",");
        self.standing(judge(&policies, call, profile), &call.tool)
    }

    /// `permission`, allowed instead when it asks only for methods a person always allows.
    fn standing(&self, permission: Permission, tool: &str) -> Result<Permission, EngineError> {
        let Permission::Ask { profile, methods, writes: false } = &permission else { return Ok(permission) };
        for &method in methods {
            let rule = StandingRule { profile: profile.clone(), tool: tool.to_owned(), method };
            if !self.approval_queue().always_allowed(&rule).map_err(|error| store_error(&error))? {
                return Ok(permission);
            }
        }
        Ok(Permission::Allow)
    }
}

/// How far an agent may go with one field: the least is the most restrictive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Refuse,
    Ask,
    Allow,
}

/// The policy's answer for `call` under `policies` (name, rules), before always-allow rules;
/// `profile` names them all for an approval.
pub fn judge(policies: &[(&str, &AgentPolicy)], call: &AgentCall, profile: String) -> Permission {
    if call.runs
        && let Some(name) = refusing(policies, |policy| if policy.run { Level::Allow } else { Level::Refuse })
    {
        return Permission::Refuse { rule: format!("{name}: agents.run is false") };
    }
    let mut methods = Vec::new();
    for &method in &call.methods {
        match strictest(policies, |policy| method_level(policy, method)) {
            (Level::Refuse, name) => {
                let rule = format!("{name}: agents.call and agents.approve leave out {}", method.name());
                return Permission::Refuse { rule };
            }
            (Level::Ask, _) if !methods.contains(&method) => methods.push(method),
            _ => {}
        }
    }
    let writes = call.writes.then(|| strictest(policies, write_level));
    if let Some((Level::Refuse, name)) = writes {
        return Permission::Refuse { rule: format!("{name}: agents.writeDefinitions is false") };
    }
    let writes = matches!(writes, Some((Level::Ask, _)));
    if methods.is_empty() && !writes { Permission::Allow } else { Permission::Ask { profile, methods, writes } }
}

/// The most restrictive level among `policies` for one field, and the first profile at it.
fn strictest<'a>(policies: &[(&'a str, &AgentPolicy)], level: impl Fn(&AgentPolicy) -> Level) -> (Level, &'a str) {
    policies
        .iter()
        .map(|(name, policy)| (level(policy), *name))
        .min_by_key(|(level, _)| *level)
        .unwrap_or((Level::Refuse, NO_PROFILE))
}

/// The first profile that refuses the field, when one does.
fn refusing<'a>(policies: &[(&'a str, &AgentPolicy)], level: impl Fn(&AgentPolicy) -> Level) -> Option<&'a str> {
    match strictest(policies, level) {
        (Level::Refuse, name) => Some(name),
        _ => None,
    }
}

fn method_level(policy: &AgentPolicy, method: Method) -> Level {
    if policy.call.contains(&method) {
        Level::Allow
    } else if policy.approve.contains(&method) {
        Level::Ask
    } else {
        Level::Refuse
    }
}

const fn write_level(policy: &AgentPolicy) -> Level {
    match policy.write_definitions {
        WriteDefinitions::Deny => Level::Refuse,
        WriteDefinitions::Ask => Level::Ask,
        WriteDefinitions::Allow => Level::Allow,
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
