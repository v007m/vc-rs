use indexmap::IndexMap;
use serde_json::{Value, json};
use vc_interfaces::DraftApi;
use vc_model::{Method, SourceFileId};
use vc_parser::{Document, Syntax};

use super::{apply, mine};
use crate::EngineError;
use crate::sync_plan::{Mine, Owned, Step};

const FILE: &str = "\
id: users
apis:
  - id: update
    method: put
    path: /users/${var.userId}
    vars:
      userId: 1
    query:
      page: ${var.page}
    headers:
      Content-Type: application/json
    operationId: updateUser
  - id: list
    path: /users
    query: {limit: '10', sort: name}
    deprecated: false
";

fn document(text: &str) -> Document {
    Document::parse(SourceFileId::new(0), Syntax::Yaml, text).unwrap()
}

fn applied(steps: &[Step]) -> Result<String, EngineError> {
    apply(document(FILE), steps).map(|document| document.text().to_owned())
}

fn put(api: &str, (map, key): (&'static str, &str), value: Value) -> Step {
    Step::Put { api: api.to_owned(), map, key: key.to_owned(), value }
}

#[test]
fn the_api_file_is_read_as_sync_sees_it() {
    let apis = mine(document(FILE).root());
    let query = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
    let update = Mine {
        id: "update".to_owned(),
        operation_id: Some("updateUser".to_owned()),
        owned: Owned {
            method: "PUT".to_owned(),
            path: "/users/${var.userId}".to_owned(),
            query: query(&[("page", "${var.page}")]),
            content_type: Some("application/json".to_owned()),
        },
        vars: vec!["userId".to_owned()],
        deprecated: false,
    };
    let list = Mine {
        id: "list".to_owned(),
        owned: Owned {
            method: "GET".to_owned(),
            path: "/users".to_owned(),
            query: query(&[("limit", "10"), ("sort", "name")]),
            content_type: None,
        },
        ..Mine::default()
    };
    assert_eq!(apis, [update, list]);
    let flagged = mine(document("apis:\n  - id: a\n    deprecated: true\n  - name: no id\n").root());
    assert_eq!(flagged.iter().map(|api| (api.id.as_str(), api.deprecated)).collect::<Vec<_>>(), [("a", true)]);
}

#[test]
fn set_replaces_a_field_or_adds_it() {
    let set = |api: &str, value: &str| Step::Set { api: api.to_owned(), key: "method", value: value.to_owned() };
    let text = applied(&[set("update", "PATCH"), set("list", "POST")]).unwrap();
    assert!(text.contains("  - id: update\n    method: PATCH\n"), "{text}");
    assert!(text.contains("    deprecated: false\n    method: POST\n"), "{text}");
}

#[test]
fn put_sets_an_entry_in_any_case_or_adds_it_and_its_map() {
    let steps = [
        put("update", ("query", "page"), json!("${var.cursor}")),
        put("update", ("headers", "content-type"), json!("application/xml")),
        put("update", ("vars", "cursor"), json!("abc")),
        put("update", ("vars", "userId"), json!(2)),
        put("list", ("vars", "limit"), json!(10)),
        put("list", ("query", "offset"), json!("${var.offset}")),
    ];
    let text = applied(&steps).unwrap();
    let after = mine(document(&text).root());
    let [update, list] = after.as_slice() else { panic!("{text}") };
    assert_eq!(update.owned.query.get("page").map(String::as_str), Some("${var.cursor}"));
    assert_eq!(update.owned.content_type.as_deref(), Some("application/xml"));
    assert!(text.contains("      Content-Type: application/xml\n"), "{text}");
    assert!(text.contains("      userId: 2\n      cursor: abc\n"), "{text}");
    assert_eq!(list.vars, ["limit"]);
    assert_eq!(list.owned.query.get("offset").map(String::as_str), Some("${var.offset}"), "{text}");
}

#[test]
fn remove_takes_an_entry_out_or_the_whole_map_with_its_last() {
    let remove = |api: &str, key: &str| Step::Remove { api: api.to_owned(), map: "query", key: key.to_owned() };
    let text = applied(&[remove("update", "page")]).unwrap();
    assert!(!text.contains("query:\n"), "{text}");
    let two = document("apis:\n  - id: a\n    query:\n      x: '1'\n      y: '2'\n");
    let kept = apply(two, &[remove("a", "x")]).unwrap();
    assert_eq!(kept.text(), "apis:\n  - id: a\n    query:\n      y: '2'\n");
}

#[test]
fn deprecate_marks_the_api_whether_or_not_it_has_the_flag() {
    let deprecate = |api: &str| Step::Deprecate { api: api.to_owned() };
    let text = applied(&[deprecate("update"), deprecate("list")]).unwrap();
    let after = mine(document(&text).root());
    assert!(after.iter().all(|api| api.deprecated), "{text}");
}

#[test]
fn append_adds_the_drafted_api_at_the_end() {
    let draft = DraftApi {
        id: "restore".to_owned(),
        method: Method::Post,
        path: "/users/${var.userId}/restore".to_owned(),
        vars: IndexMap::from([("userId".to_owned(), json!(1))]),
        operation_id: Some("restoreUser".to_owned()),
        ..DraftApi::default()
    };
    let text = applied(&[Step::Append(Box::new(draft))]).unwrap();
    let after = mine(document(&text).root());
    let restore = after.last().map(|api| (api.id.as_str(), api.owned.method.as_str(), api.operation_id.as_deref()));
    assert_eq!(restore, Some(("restore", "POST", Some("restoreUser"))), "{text}");
}

#[test]
fn a_step_that_cannot_be_made_is_refused_with_help() {
    let gone = applied(&[Step::Deprecate { api: "missing".to_owned() }]).unwrap_err();
    assert!(gone.to_string().contains("cannot sync missing"), "{gone}");
    let flow = applied(&[Step::Remove { api: "list".to_owned(), map: "query", key: "limit".to_owned() }]).unwrap_err();
    assert_eq!(flow.help().as_deref(), Some("make this change by hand, then run `vc sync` again"));
}
