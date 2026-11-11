use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_model::{ActionId, Failure};
use vc_testkit::ScriptedRandom;

use super::multipart;
use crate::action::encode::Encoding;
use crate::{BodyFiles, Unread};

/// A body as text and the header to send instead.
type Sent = (String, Option<String>);

const FORM_DATA: &str = "multipart/form-data";

/// `value` sent with `content_type` by an action whose body writes the `$file` parts the
/// engine read at `/file`, `/bin`, `/empty`, `/gone`, `/out` and `/templated`, or the failure.
fn sending(value: &Value, content_type: &str) -> Result<Sent, Failure> {
    let at = |pointer: &str| (ActionId::new(0), pointer.to_owned());
    let files = BodyFiles::new([
        (at("/file"), Ok(Bytes::from_static(&[0x89, b'P', b'N', b'G']))),
        (at("/bin"), Ok(Bytes::from_static(&[0xff, b'-', b'-', b'g', 0xfe]))),
        (at("/empty"), Ok(Bytes::new())),
        (at("/gone"), Err(Unread::Failed("no such file".to_owned()))),
        (at("/out"), Err(Unread::Refused("it leads out of the project".to_owned()))),
        (at("/templated"), Err(Unread::Templated)),
    ]);
    let random = ScriptedRandom::new([0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210]);
    let content_type = HeaderValue::from_str(content_type).unwrap();
    let encoding = Encoding { files: &files, random: &random, content_type: &content_type, action: ActionId::new(0) };
    let Value::Object(fields) = value else { return Err(Failure::new("not a map")) };
    let (bytes, header): (Bytes, _) = multipart(fields, &encoding)?;
    let header = header.map(|header| header.to_str().unwrap_or_default().to_owned());
    Ok((String::from_utf8_lossy(&bytes).into_owned(), header))
}

/// `value` sent with `content_type`, or the failure code.
fn sent(value: &Value, content_type: &str) -> Result<Sent, Option<String>> {
    sending(value, content_type).map_err(|failure| failure.code)
}

/// The V5111 message `value` fails with when sent with `content_type`.
fn failure(value: &Value, content_type: &str) -> String {
    let failure = sending(value, content_type).unwrap_err();
    assert_eq!(failure.code, Some(Code::V5111.to_string()), "{}", failure.message);
    failure.message
}

#[test]
fn text_file_and_json_parts_follow_rfc_7578_with_a_seeded_boundary() {
    let fields = json!({
        "caption": "Profile photo",
        "file": {"$file": "services/users/fixtures/avatar.png"},
        "meta": {"size": 4},
        "gone": null,
    });
    let (body, header) = sent(&fields, "multipart/form-data").unwrap();
    let boundary = "vc-0123456789abcdeffedcba98";
    assert_eq!(header.as_deref(), Some("multipart/form-data; boundary=vc-0123456789abcdeffedcba98"));
    let expected = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"caption\"\r\n\r\nProfile photo\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"avatar.png\"\r\n\
         Content-Type: image/png\r\n\r\n\u{fffd}PNG\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"meta\"\r\nContent-Type: application/json\r\n\r\n\
         {{\"size\":4}}\r\n--{boundary}--\r\n"
    );
    assert_eq!(body, expected);
}

#[test]
fn a_list_repeats_the_part_and_names_are_escaped() {
    let fields = json!({"tag\"s": ["a", 2, true]});
    let (body, _) = sent(&fields, "multipart/form-data").unwrap();
    assert_eq!(body.matches("name=\"tag%22s\"\r\n\r\n").count(), 3, "{body}");
    assert!(body.contains("\r\n\r\na\r\n") && body.contains("\r\n\r\n2\r\n") && body.contains("\r\n\r\ntrue\r\n"));
}

#[test]
fn a_boundary_the_header_names_is_used_and_the_header_kept() {
    let (body, header) = sent(&json!({"a": "1"}), "multipart/form-data; boundary=\"given-1\"").unwrap();
    assert_eq!(header, None);
    assert!(body.starts_with("--given-1\r\n") && body.ends_with("--given-1--\r\n"), "{body}");
}

#[test]
fn a_file_part_not_read_fails_with_v5111_saying_why() {
    let gone = failure(&json!({"gone": {"$file": "notes.txt"}}), FORM_DATA);
    assert_eq!(gone, "the `$file` `notes.txt` cannot be read: no such file");
    let out = failure(&json!({"out": {"$file": "../../x.txt"}}), FORM_DATA);
    assert_eq!(out, "the `$file` `../../x.txt` is refused: it leads out of the project");
    let templated = failure(&json!({"templated": {"$file": "a.bin"}}), FORM_DATA);
    assert!(templated.contains("at `/templated` cannot hold `${…}` references in its path"), "{templated}");
    let referenced = failure(&json!({"late": {"$file": "a.bin"}}), FORM_DATA);
    assert!(referenced.contains("part `late` arrives in a `${…}` value"), "{referenced}");
    for rendered in [json!({"$file": 1}), json!(5), json!({"other": "map"})] {
        let templated = failure(&json!({"templated": rendered}), FORM_DATA);
        assert!(templated.contains("at `/templated` cannot hold"), "a templated path fails whatever it renders to");
    }
    let item = failure(&json!({"file": ["x", {"$file": "a.png"}]}), FORM_DATA);
    assert!(item.contains("arrives in a"), "an item's pointer is `/name/index`, not the field's: {item}");
}

#[test]
fn a_named_boundary_must_be_valid_per_rfc_2046() {
    let named = |boundary: &str| format!("{FORM_DATA}; boundary=\"{boundary}\"");
    let wrong = |boundary: &str| failure(&json!({"a": "1"}), &named(boundary));
    assert!(wrong("").contains("it is empty"));
    assert!(wrong(&"b".repeat(71)).contains("longer than 70"));
    assert!(wrong("ab ").contains("ends in a space"));
    assert!(wrong("a@b").contains("does not allow"));
    let longest = "b".repeat(70);
    assert!(sent(&json!({"a": "1"}), &named(&longest)).is_ok());
    assert!(sent(&json!({"a": "1"}), &named("a'()+_,-./:=? z")).is_ok(), "every bchar is allowed");
}

#[test]
fn a_named_boundary_inside_a_part_fails_and_a_drawn_one_is_not_scanned() {
    let inside = failure(&json!({"note": "x\r\n--given-1\r\ny"}), "multipart/form-data; boundary=given-1");
    assert!(inside.contains("appears inside the part `note`"), "{inside}");
    let header = failure(&json!({"--given-1": "x"}), "multipart/form-data; boundary=given-1");
    assert!(header.contains("appears inside"), "a part's headers are scanned too: {header}");
    let drawn = json!({"note": "--vc-0123456789abcdeffedcba98"});
    assert!(sent(&drawn, FORM_DATA).is_ok(), "a drawn boundary is random, so it is not looked for");
}

#[test]
fn a_named_boundary_inside_a_binary_file_fails_and_an_empty_file_is_framed() {
    let inside = failure(&json!({"bin": {"$file": "blob.bin"}}), "multipart/form-data; boundary=g");
    assert!(inside.contains("appears inside the part `bin`"), "bytes that are not UTF-8 are scanned: {inside}");
    let (body, header) = sent(&json!({"empty": {"$file": "e.bin"}}), "multipart/form-data; boundary=b").unwrap();
    assert_eq!(header, None);
    let expected = "--b\r\nContent-Disposition: form-data; name=\"empty\"; filename=\"e.bin\"\r\n\
                    Content-Type: application/octet-stream\r\n\r\n\r\n--b--\r\n";
    assert_eq!(body, expected);
}
