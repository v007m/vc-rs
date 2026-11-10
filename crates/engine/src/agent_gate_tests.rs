use std::sync::Arc;

use vc_interfaces::{Approvals as _, Verdict};
use vc_model::{ActionId, ActionKind, Method, Program};
use vc_testkit::{FakeVcs, MemoryApprovals, MemoryHistory, MemorySources};

use super::{call_reach, plan_reach};
use crate::plan::Extra;
use crate::sample::builder;
use crate::{Engine, EngineError, PlanRequest, RunRequest, SelectRequest, interrupts};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n    agents: {run: true, call: [GET, PATCH], approve: [POST, DELETE]}\n  - id: bare\n",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n  - id: create\n    method: POST\n    path: /users\n    cleanup: {call: remove, vars: {userId: '${response.body.id}'}}\n  - id: remove\n    method: DELETE\n    path: /users/${var.userId}\n    vars: {userId: 0}\n  - id: touch\n    method: PATCH\n    path: /users\n",
);
const FLOW: (&str, &[u8]) = (
    "services/users/flow.test.yaml",
    b"beforeEach:\n  - call: users#touch\ntests:\n  - id: createsUser\n    call: users#create\n  - id: reusesCreate\n    steps:\n      - test: users.flow#createsUser\n      - call: users#list\n",
);

const SEED: (&str, &[u8]) = (
    "services/users/seed.test.yaml",
    b"tests:\n  - id: seedsUsers\n    steps:\n      - command: echo seeded\n      - call: users#list\n",
);

/// A project whose `users#me` signs in with the `flow` auth `login`, which sends a `PUT` and a
/// `POST`.
const LOGIN: [(&str, &[u8]); 3] = [
    (
        "vibranium.config.yaml",
        b"syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\nauth:\n  - id: login\n    type: flow\n    steps:\n      - id: token\n        method: PUT\n        url: http://id/login\n      - id: confirm\n        method: POST\n        url: http://id/confirm\n    apply:\n      headers: {authorization: 'Bearer x'}\n",
    ),
    ("services/users/users.api.yaml", b"baseUrl: ${var.baseUrl}\napis:\n  - id: me\n    path: /me\n    auth: login\n"),
    ("services/users/me.test.yaml", b"tests:\n  - id: readsMe\n    call: users#me\n"),
];

fn engine(queue: &MemoryApprovals) -> Engine {
    engine_over(&[CONFIG, USERS, FLOW, SEED], queue)
}

fn engine_over(files: &[(&str, &[u8])], queue: &MemoryApprovals) -> Engine {
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(files)))
        .history(Arc::new(MemoryHistory::new()), Arc::new(MemoryHistory::new()))
        .approvals(Arc::new(queue.clone()), Arc::new(queue.clone()))
        .build()
        .unwrap()
}

fn agent_run(profile: &str, ids: &[&str]) -> RunRequest {
    let select = SelectRequest { ids: ids.iter().map(|id| (*id).to_owned()).collect(), ..SelectRequest::default() };
    let plan = PlanRequest { select, profiles: vec![profile.to_owned()], ..PlanRequest::default() };
    RunRequest { plan, agent: Some("claude".to_owned()), ..RunRequest::default() }
}

#[test]
fn a_plan_sends_the_methods_of_every_action_it_reaches_once() {
    let engine = engine(&MemoryApprovals::new());
    let prepared = engine.prepare(&PlanRequest::default(), Extra::default()).unwrap();
    let reached = plan_reach(&prepared.program, &prepared.plan);
    assert_eq!(reached.methods, [Method::Patch, Method::Post, Method::Delete, Method::Get]);
    assert_eq!(reached.code, ["users.seed#seedsUsers: command `echo seeded`"]);
}

/// The first action that calls the api `id`.
fn calling(program: &Program, id: &str) -> ActionId {
    let calls = |kind: &ActionKind| matches!(kind, ActionKind::Call(api) if program.str(api.sym()) == Some(id));
    let index = program.actions.iter().position(|action| calls(&action.kind)).unwrap();
    ActionId::new(u32::try_from(index).unwrap())
}

#[test]
fn a_call_sends_its_method_then_its_cleanups_unless_kept() {
    let engine = engine(&MemoryApprovals::new());
    let prepared = engine.prepare(&PlanRequest::default(), Extra::default()).unwrap();
    let create = calling(&prepared.program, "users#create");
    let reached = call_reach(&prepared.program, create, false);
    assert_eq!((reached.methods, reached.code), (vec![Method::Post, Method::Delete], Vec::<String>::new()));
    assert_eq!(call_reach(&prepared.program, create, true).methods, [Method::Post]);
}

#[test]
fn a_call_and_a_run_reach_what_the_flow_auth_they_use_sends() {
    let engine = engine_over(&LOGIN, &MemoryApprovals::new());
    let prepared = engine.prepare(&PlanRequest::default(), Extra::default()).unwrap();
    let me = calling(&prepared.program, "users#me");
    for keep in [false, true] {
        let reached = call_reach(&prepared.program, me, keep);
        assert_eq!(reached.methods, [Method::Get, Method::Put, Method::Post]);
        assert_eq!(reached.code, Vec::<String>::new());
    }
    let reached = plan_reach(&prepared.program, &prepared.plan);
    assert_eq!(reached.methods, [Method::Get, Method::Put, Method::Post]);
    assert_eq!(reached.code, Vec::<String>::new());
}

#[tokio::test]
async fn an_agents_run_the_policy_refuses_runs_nothing_and_is_logged() {
    let queue = MemoryApprovals::new();
    let refused = engine(&queue).run(&agent_run("bare", &[]), interrupts().1).await;
    let Err(EngineError::Refused { reason, .. }) = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "bare: agents.run is false");
    let logged = queue.actions();
    assert_eq!(
        (logged[0].tool.as_str(), logged[0].result.as_str()),
        ("run_tests", "refused: bare: agents.run is false")
    );
}

#[tokio::test]
async fn an_agents_run_asks_once_for_every_method_its_plan_sends() {
    let queue = MemoryApprovals::answering(Verdict::Deny);
    let refused = engine(&queue).run(&agent_run("staging", &["users.flow#createsUser"]), interrupts().1).await;
    let Err(EngineError::Refused { reason, .. }) = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "request 1 was denied");
    let asked = queue.list(false).unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].request.summary, "run 1 test users.flow#createsUser");
    assert_eq!(asked[0].request.methods, [Method::Post, Method::Delete]);
    assert_eq!(asked[0].request.tool, "run_tests");
}

#[tokio::test]
async fn an_agents_run_that_reaches_a_command_asks_and_names_it() {
    let queue = MemoryApprovals::answering(Verdict::Deny);
    let refused = engine(&queue).run(&agent_run("staging", &["users.seed#seedsUsers"]), interrupts().1).await;
    let Err(EngineError::Refused { reason, .. }) = refused else { panic!("{refused:?}") };
    assert_eq!(reason, "request 1 was denied");
    let asked = queue.list(false).unwrap();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].request.methods, []);
    let summary = "run 1 test users.seed#seedsUsers; runs users.seed#seedsUsers: command `echo seeded`";
    assert_eq!(asked[0].request.summary, summary);
}

#[tokio::test]
async fn a_persons_run_is_not_gated_even_when_it_runs_code() {
    let queue = MemoryApprovals::new();
    let engine = engine(&queue);
    let request = RunRequest { agent: None, ..agent_run("staging", &["users.seed#seedsUsers"]) };
    let prepared = engine.prepare(&request.plan, Extra::default()).unwrap();
    assert!(engine.run_gate(&request, &prepared).await.unwrap().is_none());
    assert_eq!(queue.list(false).unwrap(), []);
}

#[tokio::test]
async fn an_agents_run_that_fails_before_the_gate_is_logged_and_a_persons_is_not() {
    let queue = MemoryApprovals::new();
    let engine = engine(&queue);
    let failed = engine.run(&agent_run("staging", &["users.flow#nope"]), interrupts().1).await;
    let Err(EngineError::Selection { reason, .. }) = failed else { panic!("{failed:?}") };
    let logged = queue.actions();
    assert_eq!((logged[0].tool.as_str(), logged[0].result.clone()), ("run_tests", format!("failed: {reason}")));
    assert!(logged[0].arguments.contains("users.flow#nope"), "{}", logged[0].arguments);
    let person = RunRequest { agent: None, ..agent_run("staging", &["users.flow#nope"]) };
    assert!(engine.run(&person, interrupts().1).await.is_err());
    assert_eq!(queue.actions().len(), 1);
}
