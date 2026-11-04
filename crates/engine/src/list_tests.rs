use vc_testkit::FakeVcs;

use crate::sample::engine;
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
fn the_text_and_json_carry_every_field() {
    let report = engine(FakeVcs::default()).list(&ListRequest::default()).unwrap();
    let text = report.text();
    assert!(text.contains("users.crud#listsUsers  Lists users  [smoke, fast]\n"), "{text}");
    assert!(text.ends_with("users.crud#skipsUsers  [smoke]  (skipped)\n"), "{text}");
    assert_eq!(report.json()[2]["skipped"], true);
    assert_eq!(report.json()[1]["path"], "services/users/crud.test.yaml");
}
