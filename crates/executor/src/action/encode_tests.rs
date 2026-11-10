use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Map, Value, json};
use vc_diagnostics::Code;
use vc_testkit::ScriptedRandom;

use super::{Encoding, encoder};
use crate::BodyFiles;

const FORM: &str = "application/x-www-form-urlencoded";

fn fields(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// `value` sent as a form.
fn form(value: Value) -> Result<Bytes, Option<String>> {
    let content_type = HeaderValue::from_static(FORM);
    let (files, random) = (BodyFiles::default(), ScriptedRandom::seeded(1));
    let encoding = Encoding { files: &files, random: &random, content_type: &content_type };
    let encode = encoder(&content_type).ok_or(None)?;
    let (bytes, rewritten) = encode(&fields(value), &encoding).map_err(|failure| failure.code)?;
    assert_eq!(rewritten, None, "a form's header is sent as written");
    Ok(bytes)
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
fn a_nested_object_or_list_fails_with_v5111() {
    for nested in [json!({"user": {"name": "a"}}), json!({"rows": [["a"]]})] {
        assert_eq!(form(nested), Err(Some(Code::V5111.to_string())));
    }
}
