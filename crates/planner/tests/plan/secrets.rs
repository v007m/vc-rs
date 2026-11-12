//! The secrets a run reads (D280): those the selected tests, their apis, auth and lifecycle
//! blocks read, with the run-wide vars, and not those of a test the selection leaves out.

use vc_model::{ProfileId, Program};
use vc_planner::Selection;

use crate::common::{compile_files, ids, planned};

const CONFIG: &str = "syntaxVersion: 2
profiles:
  - id: dev
    default: true
    vars: {region: '${secret.REGION_KEY}'}
  - id: prod
    vars: {token: '${secret.PROD_TOKEN}'}
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

/// The profile called `name`.
fn profile(program: &Program, name: &str) -> ProfileId {
    program.profiles.keys().copied().find(|id| program.str(id.sym()) == Some(name)).unwrap()
}

fn read(program: &Program, selection: &Selection) -> Vec<String> {
    let dev = [profile(program, "dev")];
    vc_planner::secrets(program, &planned(program, selection), &dev).into_iter().collect()
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
    let dev = [profile(&program, "dev")];
    let read: Vec<String> = vc_planner::secrets_from(&program, &[cleanup], &dev).into_iter().collect();
    assert_eq!(read, ["ADMIN_KEY", "REGION_KEY"]);
    assert_eq!(vc_planner::alert_secrets(&program).into_iter().collect::<Vec<_>>(), ["SLACK_HOOK"]);
}

#[test]
fn a_run_reads_the_profile_vars_of_only_the_profiles_it_activates() {
    let program = program();
    let selection = ids(&["users.crud#listsUsers"]);
    let plan = planned(&program, &selection);
    let prod = [profile(&program, "prod")];
    let read: Vec<String> = vc_planner::secrets(&program, &plan, &prod).into_iter().collect();
    assert_eq!(read, ["BYE_KEY", "PROD_TOKEN", "SWEEP_KEY"], "prod's vars, not dev's");
    assert!(!vc_planner::secrets(&program, &plan, &[]).contains("PROD_TOKEN"), "no profile is active");
}

const HOOKED: &str = "baseUrl: http://x
apis:
  - id: plain
    path: /plain
  - id: signed
    path: /signed
    beforeRequest:
      - function: ../../hooks/sign.ts#sign
";

const LOGGED: &str = "baseUrl: http://x
afterResponse:
  - function: ../../hooks/log.ts#log
apis:
  - id: get
    path: /get
";

const CODE_TESTS: &str = "tests:
  - id: callsPlain
    call: hooked#plain
  - id: callsSigned
    call: hooked#signed
  - id: callsLogged
    call: logged#get
  - id: checksByFunction
    call: hooked#plain
    expect:
      custom:
        - function: ../../hooks/checks.ts#isSorted
  - id: seedsByFunction
    steps:
      - id: seed
        function: ../../hooks/seed.ts#seed
";

/// A project of hooked apis whose schema declares `TOKEN` and `SIGNING_KEY`, with `config`.
fn coded(config: &str) -> Program {
    let mut program = compile_files(&[
        ("vibranium.config.yaml", config),
        ("services/hooked/hooked.api.yaml", HOOKED),
        ("services/logged/logged.api.yaml", LOGGED),
        ("services/hooked/code.test.yaml", CODE_TESTS),
    ]);
    program.config.secrets.declared = vec!["SIGNING_KEY".to_owned(), "TOKEN".to_owned()];
    program
}

fn coded_read(program: &Program, test: &str) -> Vec<String> {
    vc_planner::secrets(program, &planned(program, &ids(&[test])), &[]).into_iter().collect()
}

#[test]
fn code_in_the_selection_reads_every_declared_secret() {
    let program = coded("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n");
    assert!(coded_read(&program, "hooked.code#callsPlain").is_empty(), "pure YAML reads none");
    let all = ["SIGNING_KEY", "TOKEN"];
    assert_eq!(coded_read(&program, "hooked.code#callsSigned"), all, "an api's beforeRequest");
    assert_eq!(coded_read(&program, "hooked.code#callsLogged"), all, "an api file's afterResponse");
    assert_eq!(coded_read(&program, "hooked.code#checksByFunction"), all, "a function check");
    assert_eq!(coded_read(&program, "hooked.code#seedsByFunction"), all, "a function action");
}

#[test]
fn a_redact_function_or_a_plugin_reads_every_declared_secret_on_every_run() {
    let all = ["SIGNING_KEY", "TOKEN"];
    let redacting = coded("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nredact:\n  rules:\n    - function: hooks/redact.ts#mask\n");
    assert_eq!(coded_read(&redacting, "hooked.code#callsPlain"), all);
    let regex = coded("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nredact:\n  rules:\n    - regex: 'x+'\n");
    assert!(coded_read(&regex, "hooked.code#callsPlain").is_empty(), "a regex rule runs no code");
    let plugged = coded("syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nplugins:\n  - id: acme\n    source: path:plugins/acme\n");
    assert_eq!(coded_read(&plugged, "hooked.code#callsPlain"), all);
}
