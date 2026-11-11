use std::sync::Arc;

use vc_model::FileKind;
use vc_testkit::{FakeVcs, MemorySources};

use crate::sample::{builder, engine};
use crate::{ListKind, ListRequest, SelectRequest};

fn ids(request: &ListRequest) -> Vec<String> {
    engine(FakeVcs::default()).list(request).unwrap().items.into_iter().map(|item| item.id).collect()
}

#[test]
fn lists_each_selected_test_once_with_skipped_ones() {
    let report = engine(FakeVcs::default()).list(&ListRequest::default()).unwrap();
    let ids: Vec<&str> = report.items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["orders.flow#listsOrders", "users.crud#listsUsers", "users.crud#skipsUsers"]);
    let lists = &report.items[1];
    assert_eq!((lists.name.as_deref(), lists.path.as_str()), (Some("Lists users"), "services/users/crud.test.yaml"));
    assert_eq!(lists.tags, ["smoke", "fast"], "the file's tags, then the test's new ones");
    assert!(!lists.skipped && report.items[2].skipped);
}

#[test]
fn skipped_keeps_only_the_skipped_tests_and_selectors_apply() {
    assert_eq!(ids(&ListRequest { skipped: true, ..ListRequest::default() }), ["users.crud#skipsUsers"]);
    let select = SelectRequest { ids: vec!["orders.flow".into()], ..SelectRequest::default() };
    assert_eq!(ids(&ListRequest { select, ..ListRequest::default() }), ["orders.flow#listsOrders"]);
}

#[test]
fn lists_apis_and_files() {
    assert_eq!(ids(&ListRequest { kind: ListKind::Apis, ..ListRequest::default() }), ["users#list"]);
    let files = ids(&ListRequest { kind: ListKind::Files, ..ListRequest::default() });
    assert_eq!(files.len(), 3, "{files:?}");
    assert!(files.contains(&"users".to_owned()) && files.contains(&"users.crud".to_owned()));
}

#[test]
fn apis_and_files_take_selectors() {
    let select = |ids: &[&str], kind| ListRequest {
        kind,
        select: SelectRequest { ids: ids.iter().map(|id| (*id).to_owned()).collect(), ..SelectRequest::default() },
        ..ListRequest::default()
    };
    assert_eq!(ids(&select(&["users"], ListKind::Apis)), ["users#list"]);
    assert_eq!(ids(&select(&["users#*"], ListKind::Apis)), ["users#list"]);
    assert_eq!(ids(&select(&["users.*"], ListKind::Files)), ["users.crud"]);
    assert_eq!(ids(&select(&["services/orders"], ListKind::Files)), ["orders.flow"]);
    let error = engine(FakeVcs::default()).list(&select(&["users.crud"], ListKind::Apis)).unwrap_err();
    assert!(error.to_string().contains("no api or api file matches `users.crud`"), "{error}");
}

#[test]
fn files_filter_by_tag_and_apis_refuse_one() {
    let tagged = |kind| ListRequest {
        kind,
        select: SelectRequest { tags: vec!["smoke".into()], ..SelectRequest::default() },
        ..ListRequest::default()
    };
    assert_eq!(ids(&tagged(ListKind::Files)), ["users.crud"]);
    let error = engine(FakeVcs::default()).list(&tagged(ListKind::Apis)).unwrap_err();
    assert!(error.to_string().contains("apis have no tags"), "{error}");
}

#[test]
fn the_text_and_json_carry_every_field() {
    let report = engine(FakeVcs::default()).list(&ListRequest::default()).unwrap();
    let text = report.text();
    assert!(text.contains("users.crud#listsUsers  Lists users  [smoke, fast]\n"), "{text}");
    assert!(text.ends_with("users.crud#skipsUsers  [smoke]  (skipped)\n"), "{text}");
    assert_eq!(report.json()[2]["skipped"], true);
    assert_eq!(report.json()[1]["path"], "services/users/crud.test.yaml");
}

#[test]
fn tests_carry_their_calls_and_steps_and_apis_their_vars() {
    let config = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";
    let api = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users/${var.userId}\n    vars: {userId: '1', expand: none}\n  - id: make\n    method: POST\n    path: /users\n";
    let flow = "tests:\n  - id: makesThenGets\n    steps:\n      - command: echo hi\n      - call: users#make\n      - call: users#get\n";
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", config.as_bytes()),
        ("services/users/users.api.yaml", api.as_bytes()),
        ("services/users/flow.test.yaml", flow.as_bytes()),
    ];
    let listing = builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(&files))).build().unwrap();
    let tests = listing.list(&ListRequest::default()).unwrap();
    let flow = &tests.items[0];
    let calls: Vec<_> = flow.calls.iter().map(|call| (&*call.api, &*call.method, &*call.path)).collect();
    assert_eq!(calls, [("users#make", "POST", "/users"), ("users#get", "GET", "/users/${var.userId}")]);
    assert_eq!((flow.steps, flow.span.map(|span| span.start.line)), (3, Some(2)));
    let apis = listing.list(&ListRequest { kind: ListKind::Apis, ..ListRequest::default() }).unwrap();
    assert_eq!(apis.items[0].vars, ["userId", "expand"]);
    assert!(apis.items[1].vars.is_empty() && apis.items[1].span.is_some());
}

#[test]
fn a_test_given_no_rows_is_listed_as_skipped_not_dropped() {
    let config = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";
    let api = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users\n";
    let rows = "tests:\n  - id: listsNoPages\n    call: users#get\n    examples: []\n";
    let files: [(&str, &[u8]); 3] = [
        ("vibranium.config.yaml", config.as_bytes()),
        ("services/users/users.api.yaml", api.as_bytes()),
        ("services/users/rows.test.yaml", rows.as_bytes()),
    ];
    let listing = builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(&files))).build().unwrap();
    let tests = listing.list(&ListRequest::default()).unwrap();
    let listed: Vec<_> = tests.items.iter().map(|item| (item.id.as_str(), item.skipped)).collect();
    assert_eq!(listed, [("users.rows#listsNoPages", true)]);
    let catalog = listing.catalog().unwrap();
    assert!(catalog.items.iter().any(|item| item.id == "users.rows#listsNoPages" && item.skipped));
}

#[test]
fn the_catalog_is_the_apis_then_the_tests() {
    let catalog = engine(FakeVcs::default()).catalog().unwrap();
    let ids: Vec<&str> = catalog.items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["users#list", "orders.flow#listsOrders", "users.crud#listsUsers", "users.crud#skipsUsers"]);
    assert!(catalog.items[3].skipped);
}

#[test]
fn each_item_knows_the_kind_of_its_file() {
    let catalog = engine(FakeVcs::default()).catalog().unwrap();
    let kinds: Vec<_> = catalog.items.iter().map(super::Listed::kind).collect();
    assert_eq!(kinds, [Some(FileKind::Api), Some(FileKind::Test), Some(FileKind::Test), Some(FileKind::Test)]);
    let files = engine(FakeVcs::default()).list(&ListRequest { kind: ListKind::Files, ..ListRequest::default() });
    assert!(files.unwrap().items.iter().all(|file| file.kind().is_some()));
}
