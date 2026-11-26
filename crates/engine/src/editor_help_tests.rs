use vc_testkit::FakeVcs;

use super::{CompletionKind, EditorRequest, hover_at};
use crate::sample::engine;

const TESTS: &str = "services/users/crud.test.yaml";
const API: &str = "services/users/users.api.yaml";

/// The request for `path` with the cursor where `^` is in `text`, the `^` taken out.
fn at(path: &str, text: &str) -> EditorRequest {
    let offset = text.find('^').unwrap_or_default();
    EditorRequest { path: path.into(), text: text.replace('^', ""), offset }
}

fn labels(found: &[super::Completion]) -> Vec<&str> {
    found.iter().map(|completion| completion.label.as_str()).collect()
}

#[test]
fn completes_the_keys_a_map_has_not_got_with_their_types_and_docs() {
    let engine = engine(FakeVcs::default());
    let found = engine.complete_at(&at(TESTS, "tests:\n  - id: a\n    na^\n"));
    let [name] = found.as_slice() else { panic!("{found:?}") };
    assert!(name.label == "name" && name.kind == CompletionKind::Key && name.detail == "text", "{name:?}");
    let all = engine.complete_at(&at(TESTS, "tests:\n  - id: a\n    ^\n"));
    assert!(labels(&all).contains(&"call") && !labels(&all).contains(&"id"), "{all:?}");
    assert!(engine.complete_at(&at("notes/readme.yaml", "^")).is_empty(), "no definition kind");
}

#[test]
fn completes_values_ids_matchers_and_var_names_never_their_values() {
    let engine = engine(FakeVcs::default());
    let calls = engine.complete_at(&at(TESTS, "tests:\n  - id: a\n    call: users#^\n"));
    assert_eq!(labels(&calls), ["users#list"]);
    assert_eq!(calls[0].kind, CompletionKind::Reference);
    let methods = engine.complete_at(&at(API, "apis:\n  - id: a\n    method: PO^\n"));
    assert_eq!(labels(&methods), ["POST"]);
    let matchers = engine.complete_at(&at(API, "apis:\n  - id: a\n    expect:\n      status:\n        bet^\n"));
    assert_eq!((labels(&matchers), matchers[0].kind), (vec!["between"], CompletionKind::Matcher));
    let vars = engine.complete_at(&at(API, "apis:\n  - id: a\n    path: /x/${var.^\n"));
    let [var] = vars.as_slice() else { panic!("{vars:?}") };
    assert!(var.label == "var.baseUrl" && var.detail == "config" && !format!("{var:?}").contains("http"), "{var:?}");
}

#[test]
fn hovers_a_keys_docs_and_names_the_reference_on_its_value() {
    let key = hover_at(&at(API, "apis:\n  - id: a\n    met^hod: GET\n")).unwrap();
    assert!(key.markdown.starts_with("**`method`** · one of") && key.markdown.contains("default `GET`"), "{key:?}");
    assert_eq!(key.reference, None);
    let call = hover_at(&at(TESTS, "tests:\n  - id: a\n    call: users#l^ist\n")).unwrap();
    assert_eq!(call.reference.as_deref(), Some("users#list"));
    assert_eq!(hover_at(&at(TESTS, "tests:\n  - id: a\n    nothing^: 1\n")), None);
}
