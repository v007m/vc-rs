use std::sync::Arc;

use vc_testkit::{FakeVcs, MemorySources};

use super::{Coverage, CoveredApi};
use crate::sample::builder;
use crate::{Engine, EngineError};

const CONFIG: (&str, &[u8]) = ("vibranium.config.yaml", b"syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\n");

/// `users` comes from a spec: `list` and `get` link to its operations, `remove` to none.
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\nsource:\n  openapi: https://specs.test/users.json\napis:
  - id: list\n    path: /users\n    operationId: listUsers\n    expect: {status: 200}
  - id: get\n    path: /users/${var.userId}\n    vars: {userId: 1}\n    operationId: getUser
  - id: remove\n    method: delete\n    path: /users/1\n",
);

/// The sample clock's day is 2026-11-05: `getsThenLists` is still skipped, `listsAgain`'s skip
/// has ended. `getsThenLists` calls `users#list` twice and counts once.
const CRUD: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"tests:
  - id: listsUsers\n    call: users#list\n    expect: {status: [2xx, 200]}
  - id: getsThenLists\n    skip: {reason: flaky, until: 2026-12-01}\n    steps:
      - call: users#list\n      - call: users#get\n        expect: {status: 404}\n      - call: users#list
  - id: listsAgain\n    call: users#list\n    skip: {reason: old, until: 2026-11-01}\n",
);

/// `createUser` is an operation no api links to; `default` names no status.
const SNAPSHOT: (&str, &[u8]) = (
    "openapi/users.yaml",
    b"openapi: 3.1.0\ninfo: {title: Users, version: '1'}\npaths:
  /users:
    get: {operationId: listUsers, responses: {'200': {description: ok}, 4XX: {description: bad}, default: {description: other}}}
    post: {operationId: createUser, responses: {'201': {description: made}}}
  /users/{userId}:
    get: {operationId: getUser, responses: {'200': {description: ok}, '404': {description: none}}}\n",
);

fn engine(files: &[(&str, &[u8])]) -> Engine {
    builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(files))).build().unwrap()
}

fn row(api: &str, method: &str, path: &str, file: &str) -> CoveredApi {
    CoveredApi {
        api: api.to_owned(),
        method: method.to_owned(),
        path: path.to_owned(),
        file: file.to_owned(),
        ..CoveredApi::default()
    }
}

fn documented(coverage: &Coverage) -> Vec<(&str, Vec<&str>)> {
    let shown = |row: &CoveredApi| row.documented.iter().map(String::as_str).collect();
    coverage.apis.iter().map(|row| (row.api.as_str(), shown(row))).collect()
}

#[test]
fn each_api_counts_the_tests_that_call_it_with_the_skipped_ones_then_the_operations_no_api_links_to() {
    let coverage = engine(&[CONFIG, USERS, CRUD, SNAPSHOT]).coverage().unwrap();
    let file = "services/users/users.api.yaml";
    let expected = [
        CoveredApi { tests: 3, skipped: 1, ..row("users#list", "GET", "/users", file) },
        CoveredApi { tests: 1, skipped: 1, ..row("users#get", "GET", "/users/${var.userId}", file) },
        row("users#remove", "DELETE", "/users/1", file),
        CoveredApi { spec_only: true, ..row("createUser", "POST", "/users", "openapi/users.yaml") },
    ];
    let counted: Vec<CoveredApi> =
        coverage.apis.iter().map(|row| CoveredApi { documented: Vec::new(), ..row.clone() }).collect();
    assert_eq!(counted, expected);
    assert_eq!(coverage.notes, Vec::<String>::new());
}

#[test]
fn documented_statuses_join_the_apis_tests_steps_and_spec_each_once_with_a_class_before_its_codes() {
    let coverage = engine(&[CONFIG, USERS, CRUD, SNAPSHOT]).coverage().unwrap();
    assert_eq!(
        documented(&coverage),
        [
            ("users#list", vec!["2xx", "200", "4xx"]),
            ("users#get", vec!["200", "404"]),
            ("users#remove", vec![]),
            ("createUser", vec!["201"]),
        ]
    );
}

#[test]
fn a_missing_snapshot_of_a_synced_api_file_is_a_note_and_adds_no_rows() {
    let coverage = engine(&[CONFIG, USERS, CRUD]).coverage().unwrap();
    assert_eq!(
        coverage.apis.iter().map(|row| row.api.as_str()).collect::<Vec<_>>(),
        ["users#list", "users#get", "users#remove"]
    );
    assert_eq!(documented(&coverage)[1], ("users#get", vec!["404"]));
    let [note] = coverage.notes.as_slice() else { panic!("{coverage:?}") };
    assert!(note.contains("`openapi/users.yaml` is missing") && note.contains("`vc sync users`"), "{note}");
}

#[test]
fn a_snapshot_that_is_not_yaml_is_a_note_and_its_operations_are_left_out() {
    let broken = ("openapi/users.yaml", b"paths: [unclosed\n".as_slice());
    let coverage = engine(&[CONFIG, USERS, CRUD, broken]).coverage().unwrap();
    assert!(coverage.apis.iter().all(|row| !row.spec_only), "{coverage:?}");
    let [note] = coverage.notes.as_slice() else { panic!("{coverage:?}") };
    assert!(note.contains("`openapi/users.yaml` cannot be read as a spec"), "{note}");
}

#[test]
fn an_api_file_with_no_source_and_no_snapshot_needs_no_note() {
    let api =
        ("services/users/users.api.yaml", b"baseUrl: http://x\napis:\n  - id: list\n    path: /users\n".as_slice());
    let coverage = engine(&[CONFIG, api]).coverage().unwrap();
    assert_eq!(coverage.apis, [row("users#list", "GET", "/users", "services/users/users.api.yaml")]);
    assert_eq!(coverage.notes, Vec::<String>::new());
}

#[test]
fn a_project_that_does_not_compile_has_no_coverage() {
    let calls_nothing =
        ("services/users/crud.test.yaml", b"tests:\n  - id: getsOne\n    call: users#nope\n".as_slice());
    let refused = engine(&[CONFIG, USERS, calls_nothing]).coverage().unwrap_err();
    assert!(matches!(refused, EngineError::Invalid(_)), "{refused:?}");
}
