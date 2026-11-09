use vc_interfaces::{ImportNote, ImportOptions};
use vc_model::Method;

use super::draft;
use crate::curl::options::Request;

fn to(url: &str) -> Request {
    Request { url: Some(url.to_owned()), ..Request::default() }
}

fn opts() -> ImportOptions {
    ImportOptions { file_id: "users.admin".to_owned(), ..ImportOptions::default() }
}

#[test]
fn the_method_is_x_else_head_post_with_a_body_or_get() {
    let data = || vec!["a=1".to_owned()];
    let requests = [
        Request { method: Some("put".to_owned()), ..to("a.test/one") },
        Request { head: true, ..to("a.test/two") },
        Request { data: data(), ..to("a.test/three") },
        Request { form: data(), ..to("a.test/four") },
        Request { data: data(), get: true, ..to("a.test/five") },
    ];
    let draft = draft(&requests, &opts()).unwrap_or_default();
    let methods: Vec<_> = draft.groups.iter().flat_map(|group| &group.apis).map(|api| api.method).collect();
    assert_eq!(methods, [Method::Put, Method::Head, Method::Post, Method::Post, Method::Get]);
}

#[test]
fn the_origin_loses_its_user_and_names_the_base_url_var() {
    let draft = draft(&[to("http://ada:pw@a.test:8080?x=1")], &opts()).unwrap_or_default();
    let group = draft.groups.first().cloned().unwrap_or_default();
    assert_eq!(group.base_url.as_deref(), Some("${var.usersAdminUrl}"));
    assert_eq!(group.config_vars.get("usersAdminUrl"), Some(&serde_json::json!("http://a.test:8080")));
    let api = group.apis.first().cloned().unwrap_or_default();
    assert_eq!((api.id.as_str(), api.path.as_str()), ("get", "/"));
    let note = ImportNote {
        at: "GET /".to_owned(),
        message: "the URL's user: a credential is never written; add it with `vc new auth`".to_owned(),
    };
    assert_eq!(draft.notes, [note]);
}

#[test]
fn a_taken_id_gets_a_2_and_a_repeated_api_is_dropped() {
    let requests = [to("a.test/a-b"), to("a.test/a_b"), to("a.test/a-b?page=2")];
    let draft = draft(&requests, &opts()).unwrap_or_default();
    let ids: Vec<_> = draft.groups.iter().flat_map(|group| &group.apis).map(|api| api.id.as_str()).collect();
    assert_eq!(ids, ["getAB", "getAB2"]);
    assert_eq!(draft.notes.len(), 1);
}

#[test]
fn no_api_is_an_error_naming_the_first_note() {
    assert_eq!(draft(&[], &opts()), Err("no curl command gives an api".to_owned()));
    let requests = [Request { method: Some("FROB".to_owned()), ..to("a.test") }, to("b.test")];
    assert_eq!(draft(&requests[..1], &opts()), Err("curl 1: an api cannot use FROB".to_owned()));
    assert!(draft(&requests, &opts()).is_ok());
}
