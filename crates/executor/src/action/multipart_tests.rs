use bytes::Bytes;
use http::HeaderValue;
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_testkit::ScriptedRandom;

use super::multipart;
use crate::BodyFiles;
use crate::action::encode::Encoding;

/// A body as text and the header to send instead.
type Sent = (String, Option<String>);

/// `value` sent with `content_type`, or the failure code.
fn sent(value: &Value, content_type: &'static str) -> Result<Sent, Option<String>> {
    let files = BodyFiles::new([
        ("services/users/fixtures/avatar.png".to_owned(), Ok(vec![0x89, b'P', b'N', b'G'])),
        ("services/users/notes.txt".to_owned(), Err("services/users/notes.txt: no such file".to_owned())),
    ]);
    let (random, content_type) =
        (ScriptedRandom::new([0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210]), HeaderValue::from_static(content_type));
    let encoding = Encoding { files: &files, random: &random, content_type: &content_type };
    let Value::Object(fields) = value else { return Err(None) };
    let (bytes, header): (Bytes, _) = multipart(fields, &encoding).map_err(|failure| failure.code)?;
    let header = header.map(|header| header.to_str().unwrap_or_default().to_owned());
    Ok((String::from_utf8_lossy(&bytes).into_owned(), header))
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
fn a_file_part_not_read_fails_with_v5111() {
    let v5111 = Err(Some(Code::V5111.to_string()));
    assert_eq!(sent(&json!({"f": {"$file": "services/users/notes.txt"}}), "multipart/form-data"), v5111);
    assert_eq!(sent(&json!({"f": {"$file": "elsewhere.bin"}}), "multipart/form-data"), v5111);
}
