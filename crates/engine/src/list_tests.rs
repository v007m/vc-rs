use std::sync::Arc;

use serde_json::json;
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
    assert!(text.ends_with("users.crud#skipsUsers  [smoke]  (skipped: flaky)\n"), "{text}");
    assert_eq!(report.json()[2]["skipped"], true);
    assert_eq!((&report.json()[2]["skipReason"], &report.json()[1]["skipReason"]), (&json!("flaky"), &json!(null)));
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
    assert_eq!(tests.text(), "users.rows#listsNoPages  (skipped: examples gave no rows)\n");
    let catalog = listing.catalog(&SelectRequest::default()).unwrap();
    assert!(catalog.items.iter().any(|item| item.id == "users.rows#listsNoPages" && item.skipped));
}

#[test]
fn a_skipped_test_shows_its_reason_and_until_and_an_expired_skip_is_marked_ended() {
    let config = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";
    let api = "baseUrl: ${var.baseUrl}\napis:\n  - id: get\n    path: /users\n";
    let parked =
        "skip: {reason: auth broken in staging, until: 2026-12-01}\ntests:\n  - id: listsUsers\n    call: users#get\n";
    let own = concat!(
        "tests:\n  - id: getsUser\n    call: users#get\n    skip: {reason: 'flaky upstream, see #7', until: 2026-11-30}\n",
        "  - id: searchesUsers\n    call: users#get\n    skip: {reason: no search yet, until: 2000-01-01}\n",
    );
    let files: [(&str, &[u8]); 4] = [
        ("vibranium.config.yaml", config.as_bytes()),
        ("services/users/users.api.yaml", api.as_bytes()),
        ("services/users/parked.test.yaml", parked.as_bytes()),
        ("services/users/own.test.yaml", own.as_bytes()),
    ];
    let listing = builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(&files))).build().unwrap();
    let skipped = listing.list(&ListRequest { skipped: true, ..ListRequest::default() }).unwrap();
    let text = skipped.text();
    assert!(text.contains("users.parked#listsUsers  (skipped: auth broken in staging, until 2026-12-01)\n"), "{text}");
    assert!(text.contains("users.own#getsUser  (skipped: flaky upstream, see #7, until 2026-11-30)\n"), "{text}");
    assert!(text.contains("users.own#searchesUsers  (skip ended 2000-01-01: no search yet)\n"), "{text}");
    let json = skipped.json();
    let item = |id: &str| json.as_array().unwrap().iter().find(|item| item["id"] == id).unwrap().clone();
    let gets = item("users.own#getsUser");
    assert_eq!((&gets["skipReason"], &gets["skipUntil"]), (&json!("flaky upstream, see #7"), &json!("2026-11-30")));
    assert_eq!((&gets["skipped"], &gets["skipExpired"]), (&json!(true), &json!(false)));
    let ended = item("users.own#searchesUsers");
    assert_eq!((&ended["skipped"], &ended["skipExpired"]), (&json!(false), &json!(true)), "it runs again (V3104)");
    assert_eq!((&ended["skipReason"], &ended["skipUntil"]), (&json!("no search yet"), &json!("2000-01-01")));
    let all = listing.list(&ListRequest::default()).unwrap();
    let searches = all.items.iter().find(|item| item.id == "users.own#searchesUsers").unwrap();
    assert!(!searches.skipped && searches.skip_expired, "listed with the rest, as a test that runs");
}

#[test]
fn the_catalog_is_the_apis_then_the_tests() {
    let catalog = engine(FakeVcs::default()).catalog(&SelectRequest::default()).unwrap();
    let ids: Vec<&str> = catalog.items.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["users#list", "orders.flow#listsOrders", "users.crud#listsUsers", "users.crud#skipsUsers"]);
    assert!(catalog.items[3].skipped);
}

#[test]
fn each_item_knows_the_kind_of_its_file() {
    let catalog = engine(FakeVcs::default()).catalog(&SelectRequest::default()).unwrap();
    let kinds: Vec<_> = catalog.items.iter().map(super::Listed::kind).collect();
    assert_eq!(kinds, [Some(FileKind::Api), Some(FileKind::Test), Some(FileKind::Test), Some(FileKind::Test)]);
    let files = engine(FakeVcs::default()).list(&ListRequest { kind: ListKind::Files, ..ListRequest::default() });
    assert!(files.unwrap().items.iter().all(|file| file.kind().is_some()));
}

#[test]
fn the_catalog_keeps_what_the_selection_picks_and_an_empty_one_is_no_error() {
    let window = Some(std::time::Duration::from_hours(7 * 24));
    let select = SelectRequest { changed_since: window, ..SelectRequest::default() };
    let ids = |vcs: FakeVcs| {
        let catalog = engine(vcs).catalog(&select).unwrap();
        catalog.items.iter().map(|item| item.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(ids(FakeVcs::default().with_recent(&["services/orders/flow.test.yaml"])), ["orders.flow#listsOrders"]);
    let api = ids(FakeVcs::default().with_recent(&["services/users/users.api.yaml"]));
    assert_eq!(api.first().map(String::as_str), Some("users#list"), "the api, then the tests that call it: {api:?}");
    assert_eq!(ids(FakeVcs::default()), Vec::<String>::new());
}

#[test]
fn files_carry_the_name_each_file_declares() {
    let config = "syntaxVersion: 2\nvars: {baseUrl: 'http://x'}\nprofiles:\n  - id: dev\n    default: true\n";
    let api = "name: Users service\nbaseUrl: ${var.baseUrl}\napis:\n  - id: list\n    path: /users\n";
    let crud = "name: User CRUD\ntests:\n  - id: listsUsers\n    call: users#list\n";
    let bare = "tests:\n  - id: listsAgain\n    call: users#list\n";
    let peak = "name: Peak traffic\nmix:\n  - {test: users.crud#listsUsers, weight: 1}\nstages:\n  - {duration: 3s, users: 2}\n";
    let files: [(&str, &[u8]); 5] = [
        ("vibranium.config.yaml", config.as_bytes()),
        ("services/users/users.api.yaml", api.as_bytes()),
        ("services/users/crud.test.yaml", crud.as_bytes()),
        ("services/users/bare.test.yaml", bare.as_bytes()),
        ("load/users/peak.load.yaml", peak.as_bytes()),
    ];
    let listing = builder(FakeVcs::default()).sources(Arc::new(MemorySources::new(&files))).build().unwrap();
    let report = listing.list(&ListRequest { kind: ListKind::Files, ..ListRequest::default() }).unwrap();
    let mut named: Vec<_> = report.items.iter().map(|item| (item.id.as_str(), item.name.as_deref())).collect();
    named.sort_unstable();
    let expected = [
        ("users", Some("Users service")),
        ("users.bare", None),
        ("users.crud", Some("User CRUD")),
        ("users.peak", Some("Peak traffic")),
    ];
    assert_eq!(named, expected);
    let json = report.json();
    let crud = json.as_array().unwrap().iter().find(|item| item["id"] == "users.crud").unwrap();
    assert_eq!(crud["name"], "User CRUD", "not null, as it was before");
    assert!(report.text().contains("users.crud  User CRUD\n"), "{}", report.text());
}
