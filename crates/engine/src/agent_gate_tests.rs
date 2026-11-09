use std::sync::Arc;

use vc_interfaces::{Approvals as _, Verdict};
use vc_model::Method;
use vc_testkit::{FakeVcs, MemoryApprovals, MemoryHistory, MemorySources};

use super::{api_methods, plan_methods};
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

fn engine(queue: &MemoryApprovals) -> Engine {
    builder(FakeVcs::default())
        .sources(Arc::new(MemorySources::new(&[CONFIG, USERS, FLOW])))
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
    let methods = plan_methods(&prepared.program, &prepared.plan);
    assert_eq!(methods, [Method::Patch, Method::Post, Method::Delete, Method::Get]);
}

#[test]
fn an_api_sends_its_method_then_its_cleanups() {
    let engine = engine(&MemoryApprovals::new());
    let prepared = engine.prepare(&PlanRequest::default(), Extra::default()).unwrap();
    let program = &prepared.program;
    let create = program.apis.values().find(|api| program.str(api.id.sym()) == Some("users#create")).unwrap();
    assert_eq!(api_methods(program, create.id), [Method::Post, Method::Delete]);
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
