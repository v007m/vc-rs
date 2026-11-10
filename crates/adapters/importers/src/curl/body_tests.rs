use indexmap::IndexMap;
use serde_json::{Value, json};

use super::{Body, body};
use crate::curl::options::Request;

fn sent(data: &str, content_type: &str) -> Request {
    let headers = vec![("Content-Type".to_owned(), content_type.to_owned())];
    Request { data: vec![data.to_owned()], headers, ..Request::default() }
}

fn read(request: &Request) -> (Body, Vec<String>) {
    let taken = IndexMap::from([("userId".to_owned(), json!("u_1"))]);
    let mut found = Vec::new();
    (body(request, &taken, &mut found), found)
}

fn value(body: &Body) -> Option<&Value> {
    body.value.as_ref()
}

#[test]
fn json_fields_become_vars_unless_their_name_is_taken() {
    let (body, found) = read(&sent(r#"{"user_id":"u_2","age":7,"meta":{"apiKey":"k1"}}"#, "application/json"));
    assert_eq!(value(&body), Some(&json!({"user_id": "u_2", "age": "${var.age}", "meta": "${var.meta}"})));
    let vars = IndexMap::from([("age".to_owned(), json!(7)), ("meta".to_owned(), json!({"apiKey": ""}))]);
    assert_eq!(body.vars, vars);
    assert_eq!(found, ["body field apiKey: a credential is never written, so it is empty"]);
}

#[test]
fn json_that_is_not_an_object_or_not_json_is_kept() {
    let (body, found) = read(&sent(r#"[{"token":"t1"},2]"#, "application/vnd.api+json"));
    assert_eq!((value(&body), body.vars.len()), (Some(&json!([{"token": ""}, 2])), 0));
    assert_eq!(found.len(), 1);
    let (body, found) = read(&Request { json: true, ..sent("{oops", "application/json; charset=utf-8") });
    assert_eq!(value(&body), Some(&json!("{oops")));
    assert_eq!(body.content_type.as_deref(), Some("application/json; charset=utf-8"));
    assert_eq!(found, ["the body is not valid JSON, so it is kept as text"]);
}

#[test]
fn data_is_a_form_by_default_json_with_json_and_text_otherwise() {
    let joined = Request { data: vec!["a=1".to_owned(), "b=x%20y".to_owned()], ..Request::default() };
    let (body, _) = read(&joined);
    assert_eq!(value(&body), Some(&json!({"a": "1", "b": "x y"})));
    assert_eq!(body.content_type.as_deref(), Some("application/x-www-form-urlencoded"));
    let (body, _) = read(&Request { data: vec!["{}".to_owned()], json: true, ..Request::default() });
    assert_eq!(body.content_type.as_deref(), Some("application/json"));
    let (body, found) = read(&sent("hello", "text/plain"));
    assert_eq!((value(&body), found.len()), (Some(&json!("hello")), 0));
}

#[test]
fn get_or_no_data_sends_no_body() {
    assert_eq!(read(&Request { get: true, ..sent("a=1", "text/plain") }).0, Body::default());
    assert_eq!(read(&Request::default()).0, Body::default());
}
