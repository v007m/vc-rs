//! A compiled sample project and helpers to read plans of it by name.
//!
//! Files plan in path order (`legacy`, `orders`, `users`). Tests: `users.crud` (`listsUsers` ×2, `createsUser` tagged `slow` and reading `seed`,
//! `readsWarm` reading `warm`), `orders.checkout` (`paysOrder` reading `cold` and `prime`,
//! `refundsOrder` skipped until 2099, `voidsOrder` skipped until 2000) and `legacy.old`, a skipped
//! file with `runsOld`. Today is 2026-11-03.

use std::sync::Arc;

use vc_compiler::CompileInput;
use vc_model::{ActionId, ContentHash, Plan, Program, SourceFile, SourcePath, Sym, TestId};
use vc_planner::{PlanError, PlanOptions, Selection};

pub const CONFIG: &str = "syntaxVersion: 2
profiles:
  - id: dev
    default: true
vars: {baseUrl: 'http://x'}
beforeRun:
  - id: warm
    call: users#list
  - id: cold
    call: orders#list
afterRun:
  - id: bye
    call: users#list
";

pub const USERS: &str = "name: Users
baseUrl: ${var.baseUrl}
apis:
  - id: list
    path: /users
    expect: {status: 200}
    save: {token: '${response.body.token}'}
  - id: create
    method: POST
    path: /users
    body: {name: Ada}
    expect: {status: 201}
    save: {userId: '${response.body.id}'}
";

const ORDERS: &str = "name: Orders
baseUrl: ${var.baseUrl}
apis:
  - id: list
    path: /orders
    save: {token: '${response.body.token}'}
  - id: pay
    method: POST
    path: /orders/pay
    expect: {status: 2xx}
";

const CRUD: &str = "name: Crud
tags: [smoke]
beforeAll:
  - id: seed
    call: users#create
beforeEach:
  - call: users#list
tests:
  - id: listsUsers
    call: users#list
    repeat: 2
  - id: createsUser
    call: users#create
    tags: [slow]
    headers: {x-seed: '${var.seed.userId}'}
  - id: readsWarm
    call: users#list
    headers: {x-warm: '${var.warm.token}'}
";

const CHECKOUT: &str = "name: Checkout
tags: [checkout]
beforeAll:
  - id: prime
    call: orders#list
tests:
  - id: paysOrder
    call: orders#pay
    headers: {x-cold: '${var.cold.token}', x-prime: '${var.prime.token}'}
  - id: refundsOrder
    call: orders#pay
    tags: [slow]
    skip: {reason: flaky, until: '2099-01-01'}
  - id: voidsOrder
    call: orders#pay
    skip: {reason: gone, until: '2000-01-01'}
";

const OLD: &str = "name: Old
skip: {reason: retired}
tests:
  - id: runsOld
    call: users#list
    skip: {reason: own}
";

/// Compiles `files`, every test standalone.
pub fn compile_files(files: &[(&str, &str)]) -> Program {
    let files = files.iter().map(|(path, text)| source(path, text)).collect();
    let compiled = vc_compiler::compile(CompileInput { files, ..CompileInput::default() });
    let Some(program) = compiled.program else { panic!("{:?}", compiled.diagnostics) };
    let mut program = Arc::unwrap_or_clone(program);
    // The compiler reads a missing `standalone` as false (with m1-lint); remove when that lands.
    for test in program.tests.values_mut() {
        test.standalone = true;
    }
    program
}

fn source(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The sample project, with `extra` files added.
pub fn project_with(extra: &[(&str, &str)]) -> Program {
    let mut files = vec![
        ("vibranium.config.yaml", CONFIG),
        ("services/users/users.api.yaml", USERS),
        ("services/orders/orders.api.yaml", ORDERS),
        ("services/users/crud.test.yaml", CRUD),
        ("services/orders/checkout.test.yaml", CHECKOUT),
        ("services/legacy/old.test.yaml", OLD),
    ];
    files.extend_from_slice(extra);
    compile_files(&files)
}

pub fn project() -> Program {
    project_with(&[])
}

pub fn today() -> PlanOptions {
    PlanOptions { today: Some("2026-11-03".into()), ..PlanOptions::default() }
}

pub fn try_plan(program: &Program, selection: &Selection) -> Result<Plan, PlanError> {
    vc_planner::plan(program, selection, &today())
}

pub fn planned(program: &Program, selection: &Selection) -> Plan {
    try_plan(program, selection).unwrap()
}

pub fn ids(ids: &[&str]) -> Selection {
    Selection { ids: ids.iter().map(|id| (*id).to_owned()).collect(), ..Selection::default() }
}

pub fn name(program: &Program, sym: Sym) -> &str {
    program.str(sym).unwrap()
}

pub fn test_id(program: &Program, full_id: &str) -> TestId {
    *program.tests.keys().find(|test| name(program, test.sym()) == full_id).unwrap()
}

/// Each planned test once, by short ID, in plan order.
pub fn tests<'p>(program: &'p Program, plan: &Plan) -> Vec<&'p str> {
    let mut tests: Vec<&str> = Vec::new();
    for instance in plan.files.iter().flat_map(|file| &file.instances) {
        let short = short(name(program, instance.test.sym()));
        if !tests.contains(&short) {
            tests.push(short);
        }
    }
    tests
}

pub fn skipped<'p>(program: &'p Program, plan: &Plan) -> Vec<&'p str> {
    plan.skipped.iter().map(|skipped| short(name(program, skipped.test.sym()))).collect()
}

/// Each instance of `full_id` as (example row, repeat index).
pub fn slots(program: &Program, plan: &Plan, full_id: &str) -> Vec<(Option<u32>, u32)> {
    let test = test_id(program, full_id);
    let instances = plan.files.iter().flat_map(|file| &file.instances).filter(|instance| instance.test == test);
    instances.map(|instance| (instance.example_row, instance.repeat_index)).collect()
}

pub fn actions(program: &Program, actions: impl IntoIterator<Item = ActionId>) -> Vec<&str> {
    let named = |action: ActionId| program.actions[usize::try_from(action.index()).unwrap()].id;
    actions.into_iter().map(|action| named(action).map_or("?", |sym| name(program, sym))).collect()
}

fn short(full_id: &str) -> &str {
    full_id.rsplit_once('#').map_or(full_id, |(_, id)| id)
}
