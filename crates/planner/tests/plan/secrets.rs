//! The secrets a run reads (D280): those the selected tests, their apis, auth and lifecycle
//! blocks read, with the run-wide vars, and not those of a test the selection leaves out.

use vc_model::Program;
use vc_planner::Selection;

use crate::common::{compile_files, ids, planned};

const CONFIG: &str = "syntaxVersion: 2
profiles:
  - id: dev
    default: true
    vars: {region: '${secret.REGION_KEY}'}
vars: {baseUrl: 'http://x'}
auth:
  - id: partner
    type: bearer
    token: ${secret.PARTNER_TOKEN}
beforeRun:
  - id: warm
    call: users#list
afterRun:
  - id: bye
    call: users#list
    headers: {x-bye: '${secret.BYE_KEY}'}
alerts:
  - on: [failure]
    notify:
      - slack: {webhook: '${secret.SLACK_HOOK}'}
";

const USERS: &str = "baseUrl: ${var.baseUrl}
apis:
  - id: list
    path: /users
  - id: create
    method: POST
    path: /users
    headers: {x-api-key: '${secret.USERS_KEY}'}
    body: {name: Ada}
    cleanup:
      call: remove
  - id: remove
    method: DELETE
    path: /users/1
    headers: {x-admin: '${secret.ADMIN_KEY}'}
";

const PARTNER: &str = "baseUrl: ${var.baseUrl}
auth: partner
apis:
  - id: quote
    path: /quote
";

const CRUD: &str = "afterAll:
  - id: sweep
    call: users#list
    query: {key: '${secret.SWEEP_KEY}'}
tests:
  - id: createsUser
    call: users#create
  - id: listsUsers
    call: users#list
";

const QUOTES: &str = "tests:
  - id: getsQuote
    call: partner#quote
    vars: {note: '${secret.NOTE_KEY}'}
";

fn program() -> Program {
    compile_files(&[
        ("vibranium.config.yaml", CONFIG),
        ("services/users/users.api.yaml", USERS),
        ("services/partner/partner.api.yaml", PARTNER),
        ("services/users/crud.test.yaml", CRUD),
        ("services/partner/quotes.test.yaml", QUOTES),
    ])
}

fn read(program: &Program, selection: &Selection) -> Vec<String> {
    vc_planner::secrets(program, &planned(program, selection)).into_iter().collect()
}

#[test]
fn a_run_reads_the_secrets_of_what_it_selects_and_the_run_wide_ones_only() {
    let program = program();
    let users = ["ADMIN_KEY", "BYE_KEY", "REGION_KEY", "SWEEP_KEY", "USERS_KEY"];
    assert_eq!(read(&program, &ids(&["users.crud#createsUser"])), users, "an api, its cleanup and afterAll");
    let listing = ["BYE_KEY", "REGION_KEY", "SWEEP_KEY"];
    assert_eq!(read(&program, &ids(&["users.crud#listsUsers"])), listing, "createsUser's api is not reached");
    let quote = ["BYE_KEY", "NOTE_KEY", "PARTNER_TOKEN", "REGION_KEY"];
    assert_eq!(read(&program, &ids(&["partner.quotes"])), quote, "the api file's auth and the test's vars");
}

#[test]
fn a_call_reads_its_actions_secrets_and_the_alerts_read_their_channels() {
    let program = program();
    let create = program.apis.values().find(|api| program.str(api.id.sym()) == Some("users#create")).unwrap();
    let cleanup = create.cleanup.unwrap();
    let read: Vec<String> = vc_planner::secrets_from(&program, &[cleanup]).into_iter().collect();
    assert_eq!(read, ["ADMIN_KEY", "REGION_KEY"]);
    assert_eq!(vc_planner::alert_secrets(&program).into_iter().collect::<Vec<_>>(), ["SLACK_HOOK"]);
}
