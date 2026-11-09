use vc_interfaces::{ApprovalRequest, Approvals, Decision, Verdict};
use vc_model::{AgentPolicy, Method, WriteDefinitions};
use vc_testkit::MemoryApprovals;

use super::{AgentCall, Permission, judge};
use crate::sample::agent_engine;

fn sends(profiles: &[&str], methods: &[Method]) -> AgentCall {
    AgentCall {
        agent: "claude".into(),
        tool: "call_api".into(),
        profiles: profiles.iter().map(|&profile| profile.to_owned()).collect(),
        methods: methods.to_vec(),
        ..AgentCall::default()
    }
}

fn policy(run: bool, call: &[Method], approve: &[Method], write_definitions: WriteDefinitions) -> AgentPolicy {
    AgentPolicy { run, call: call.to_vec(), approve: approve.to_vec(), write_definitions }
}

fn judged(policies: &[(&str, &AgentPolicy)], call: &AgentCall) -> Permission {
    judge(policies, call, "p".into())
}

fn ask(methods: &[Method], writes: bool) -> Permission {
    Permission::Ask { profile: "p".into(), methods: methods.to_vec(), writes }
}

fn refuse(rule: &str) -> Permission {
    Permission::Refuse { rule: rule.into() }
}

#[test]
fn a_read_goes_ahead_under_any_policy() {
    let none = AgentPolicy::default();
    assert_eq!(judged(&[("bare", &none)], &AgentCall::default()), Permission::Allow);
}

#[test]
fn a_run_needs_agents_run() {
    let call = AgentCall { runs: true, ..AgentCall::default() };
    let open = policy(true, &[], &[], WriteDefinitions::Deny);
    assert_eq!(judged(&[("dev", &open)], &call), Permission::Allow);
    let closed = AgentPolicy::default();
    assert_eq!(judged(&[("dev", &open), ("prod", &closed)], &call), refuse("prod: agents.run is false"));
}

#[test]
fn each_method_is_allowed_asked_or_refused() {
    let staging = policy(true, &[Method::Get], &[Method::Delete], WriteDefinitions::Deny);
    let only = |methods: &[Method]| judged(&[("staging", &staging)], &sends(&[], methods));
    assert_eq!(only(&[Method::Get]), Permission::Allow);
    assert_eq!(only(&[Method::Get, Method::Delete, Method::Delete]), ask(&[Method::Delete], false));
    let left_out = refuse("staging: agents.call and agents.approve leave out PATCH");
    assert_eq!(only(&[Method::Delete, Method::Patch]), left_out);
}

#[test]
fn the_most_restrictive_profile_wins_for_each_method() {
    let dev = policy(true, &[Method::Get, Method::Post], &[], WriteDefinitions::Allow);
    let staging = policy(true, &[Method::Get], &[Method::Post], WriteDefinitions::Allow);
    let both = [("dev", &dev), ("staging", &staging)];
    assert_eq!(judged(&both, &sends(&[], &[Method::Get])), Permission::Allow);
    assert_eq!(judged(&both, &sends(&[], &[Method::Post])), ask(&[Method::Post], false));
}

#[test]
fn a_write_needs_write_definitions() {
    let write = AgentCall { writes: true, ..AgentCall::default() };
    let level = |write_definitions| policy(false, &[], &[], write_definitions);
    let (allow, ask_first, deny) =
        (level(WriteDefinitions::Allow), level(WriteDefinitions::Ask), level(WriteDefinitions::Deny));
    assert_eq!(judged(&[("dev", &allow)], &write), Permission::Allow);
    assert_eq!(judged(&[("dev", &allow), ("staging", &ask_first)], &write), ask(&[], true));
    assert_eq!(
        judged(&[("staging", &ask_first), ("prod", &deny)], &write),
        refuse("prod: agents.writeDefinitions is false")
    );
}

#[test]
fn the_engine_judges_under_the_active_profiles() {
    let engine = agent_engine(&MemoryApprovals::new());
    let check = |profiles: &[&str], methods: &[Method]| engine.policy_check(&sends(profiles, methods)).unwrap();
    assert_eq!(check(&[], &[Method::Delete]), Permission::Allow);
    let staging = Permission::Ask { profile: "staging".into(), methods: vec![Method::Delete], writes: false };
    assert_eq!(check(&["staging"], &[Method::Delete]), staging);
    let both = Permission::Ask { profile: "dev,prod".into(), methods: vec![Method::Post], writes: false };
    assert_eq!(check(&["dev,prod"], &[Method::Post]), both);
    let bare = refuse("bare: agents.call and agents.approve leave out GET");
    assert_eq!(check(&["bare"], &[Method::Get]), bare);
    let run = AgentCall { runs: true, profiles: vec!["prod".into()], ..AgentCall::default() };
    assert_eq!(engine.policy_check(&run).unwrap(), refuse("prod: agents.run is false"));
}

#[test]
fn an_always_allow_lets_the_method_through_for_that_profile_and_tool() {
    let queue = MemoryApprovals::new();
    let engine = agent_engine(&queue);
    let request = ApprovalRequest {
        agent: "claude".into(),
        tool: "call_api".into(),
        profile: "staging".into(),
        methods: vec![Method::Delete],
        summary: "DELETE users#remove".into(),
        arguments: "{}".into(),
    };
    let id = queue.create(&request).unwrap();
    queue.decide(&Decision { id, verdict: Verdict::AlwaysAllow, by: "sam".into() }).unwrap();
    assert_eq!(engine.policy_check(&sends(&["staging"], &[Method::Delete])).unwrap(), Permission::Allow);
    let other_tool = AgentCall { tool: "run_tests".into(), ..sends(&["staging"], &[Method::Delete]) };
    assert!(matches!(engine.policy_check(&other_tool).unwrap(), Permission::Ask { .. }));
    let put = sends(&["staging"], &[Method::Delete, Method::Put]);
    assert!(matches!(engine.policy_check(&put).unwrap(), Permission::Ask { .. }), "PUT still asks");
}
