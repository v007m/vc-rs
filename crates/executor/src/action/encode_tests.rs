use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Map, Value, json};
use vc_diagnostics::Code;
use vc_model::ActionId;
use vc_testkit::ScriptedRandom;

use super::{Encoding, encoder, items};
use crate::BodyFiles;

const FORM: &str = "application/x-www-form-urlencoded";

fn fields(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// `value` sent as a form, or the failure's code and message.
fn sent(value: Value) -> Result<Bytes, (Option<String>, String)> {
    let content_type = HeaderValue::from_static(FORM);
    let (files, random) = (BodyFiles::default(), ScriptedRandom::seeded(1));
    let encoding = Encoding { files: &files, random: &random, content_type: &content_type, action: ActionId::new(0) };
    let encode = encoder(&content_type).ok_or((None, String::new()))?;
    let encoded = encode(&fields(value), &encoding).map_err(|failure| (failure.code, failure.message));
    let (bytes, rewritten) = encoded?;
    assert_eq!(rewritten, None, "a form's header is sent as written");
    Ok(bytes)
}

/// `value` sent as a form, or the failure code.
fn form(value: Value) -> Result<Bytes, Option<String>> {
    sent(value).map_err(|(code, _)| code)
}

#[test]
fn the_form_encoder_is_chosen_by_media_type_whatever_its_case_or_parameters() {
    let chosen = |value: &'static str| encoder(&HeaderValue::from_static(value)).is_some();
    assert!(chosen(FORM));
    assert!(chosen("Application/X-WWW-Form-URLEncoded ; charset=utf-8"));
    assert!(!chosen("application/json"));
    assert!(!chosen("application/x-www-form-urlencoded-not"));
    assert!(chosen("Multipart/Form-Data; boundary=x"));
}

#[test]
fn a_form_is_whatwg_encoded_with_space_as_plus() {
    let sent = form(json!({"a": "one two", "b": 2, "c&d": "x+y=z*-._~é", "e": true}));
    assert_eq!(sent, Ok(Bytes::from("a=one+two&b=2&c%26d=x%2By%3Dz*-._%7E%C3%A9&e=true")));
}

#[test]
fn a_list_repeats_its_key_and_a_null_is_left_out() {
    let sent = form(json!({"empty": "", "gone": null, "tag": ["a", null, "b"]}));
    assert_eq!(sent, Ok(Bytes::from("empty=&tag=a&tag=b")), "keys in order either way: preserve_order may be off");
}

#[test]
fn a_nested_object_or_list_fails_with_v5111_naming_what_it_holds() {
    let v5111 = Some(Code::V5111.to_string());
    let holds =
        |what: &str, key: &str| format!("the form field `{key}` holds {what}; a form field takes text or a list");
    assert_eq!(sent(json!({"user": {"name": "a"}})), Err((v5111.clone(), holds("an object", "user"))));
    assert_eq!(sent(json!({"rows": [["a"]]})), Err((v5111, holds("a list", "rows"))), "a list inside a list");
}

#[test]
fn items_keep_their_index_in_the_list_and_leave_nulls_out() {
    let listed = json!(["a", null, "b"]);
    let found: Vec<(Option<usize>, &Value)> = items(&listed).collect();
    assert_eq!(found, [(Some(0), &json!("a")), (Some(2), &json!("b"))]);
    assert_eq!(items(&json!("x")).collect::<Vec<_>>(), [(None, &json!("x"))], "a value is not a list item");
    assert_eq!(items(&Value::Null).count(), 0);
}
