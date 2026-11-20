//! The AWS Signature Version 4 test suite (D441), vendored unchanged in `tests/sigv4/` with its
//! licence and source commit. Every case runs, except the ones in [`SKIPPED`], each with why.

use std::fs;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use serde_json::{Map, Value, json};
use vc_interfaces::AuthRequest;
use vc_model::RenderedRequest;

use super::crypto::Moment;
use super::{CONTENT, SigV4, canonical, string_to_sign};

/// The bytes of a request line that a URL cannot hold raw, encoded as a rendered URL has them
/// (D278); `percent_encode` encodes every byte outside ASCII too.
const RAW: &AsciiSet = &CONTROLS.add(b' ');

/// Every case's time, `2015-08-30T12:36:00Z`, in milliseconds since the Unix epoch.
pub const SUITE_MS: u64 = 1_440_938_160_000;
const SUITE_TIME: &str = "2015-08-30T12:36:00Z";

/// The cases that do not run, and why.
const SKIPPED: [(&str, &str); 5] = [
    (
        "get-space-normalized",
        "the raw space in the path is signed unencoded; a rendered path is already encoded once (D278), so \
         double encoding gives `%2520`, as AWS does for that wire path",
    ),
    (
        "get-utf8",
        "the raw UTF-8 in the path is signed unencoded; a rendered path is already encoded once (D278), so \
         double encoding gives `%25E1`, as AWS does for that wire path",
    ),
    ("post-sts-header-after", "`omit_session_token` adds the token after signing, which `awsSigV4` does not offer"),
    ("post-x-www-form-urlencoded", "it signs `content-length`, which D435 leaves out because the transport sets it"),
    (
        "post-x-www-form-urlencoded-parameters",
        "it signs `content-length`, which D435 leaves out because the transport sets it",
    ),
];

fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sigv4")
}

fn cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> =
        fs::read_dir(suite()).unwrap().map(|entry| entry.unwrap().path()).filter(|path| path.is_dir()).collect();
    cases.sort();
    cases
}

fn read(case: &Path, file: &str) -> String {
    fs::read_to_string(case.join(file)).unwrap()
}

/// The signer for a case's context: S3's rules when it does not normalise.
fn signer(context: &Value) -> SigV4 {
    let credentials = &context["credentials"];
    let mut settings = json!({
        "region": context["region"],
        "service": context["service"],
        "accessKeyId": credentials["access_key_id"],
        "secretAccessKey": credentials["secret_access_key"],
    });
    if let Some(token) = credentials.get("token") {
        settings["sessionToken"] = token.clone();
    }
    let settings: Map<String, Value> = settings.as_object().cloned().unwrap();
    let mut signer = SigV4::from_request(&AuthRequest::new("aws", "awsSigV4", settings)).unwrap();
    signer.s3 = context["normalize"] == json!(false);
    signer
}

/// `request.txt` as a rendered request. A line that starts with whitespace continues the header
/// before it, joined with a space (RFC 9112 section 5.2); the head's last newline ends no header.
fn parse(text: &str) -> RenderedRequest {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut lines = head.split('\n');
    let (method, rest) = lines.next().unwrap().split_once(' ').unwrap();
    let (target, _version) = rest.rsplit_once(' ').unwrap();
    let mut pairs: Vec<(String, String)> = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        match pairs.last_mut() {
            Some((_, value)) if line.starts_with([' ', '\t']) => {
                value.push(' ');
                value.push_str(line);
            }
            _ => {
                let (name, value) = line.split_once(':').unwrap();
                pairs.push((name.to_owned(), value.to_owned()));
            }
        }
    }
    let mut headers = HeaderMap::new();
    for (name, value) in pairs {
        headers.append(HeaderName::from_bytes(name.as_bytes()).unwrap(), HeaderValue::from_str(&value).unwrap());
    }
    let target = percent_encode(target.as_bytes(), RAW).to_string();
    let uri = Uri::try_from(format!("https://{}{target}", headers["host"].to_str().unwrap())).unwrap();
    let method = Method::from_bytes(method.as_bytes()).unwrap();
    RenderedRequest { method, uri, headers, body: Bytes::from(body.to_owned()) }
}

/// The `Authorization` line of `header-signed-request.txt`.
fn expected_authorization(case: &Path) -> String {
    let signed = read(case, "header-signed-request.txt");
    signed.lines().find_map(|line| line.strip_prefix("Authorization:")).unwrap().to_owned()
}

/// Signs one case and compares each step with the suite's files. `x-amz-content-sha256` is
/// taken off before the canonical request is made unless the case signs it (`sign_body`).
fn check(case: &Path) {
    let name = case.file_name().unwrap().to_string_lossy().into_owned();
    let context: Value = serde_json::from_str(&read(case, "context.json")).unwrap();
    assert_eq!(context["timestamp"], SUITE_TIME, "{name}: the suite's time");
    let signer = signer(&context);
    let moment = Moment::at(SUITE_MS);
    let payload = signer.payload_hash(&parse(&read(case, "request.txt")).body);
    let mut request = signer.stamp(parse(&read(case, "request.txt")), &moment, &payload).unwrap();
    if context["sign_body"] != json!(true) {
        request.headers.remove(CONTENT);
    }
    let canonical = canonical::request(&request, &payload, signer.s3);
    let text = String::from_utf8(canonical.text().to_vec()).unwrap();
    assert_eq!(text, read(case, "header-canonical-request.txt"), "{name}: the canonical request");
    let to_sign = string_to_sign(&moment.stamp, &signer.scope(&moment.date), canonical.text());
    assert_eq!(to_sign, read(case, "header-string-to-sign.txt"), "{name}: the string to sign");
    assert_eq!(signer.signature(&moment.date, &to_sign), read(case, "header-signature.txt"), "{name}: the signature");
    let authorization = signer.authorization(&canonical, &moment).unwrap();
    assert_eq!(authorization.to_str().unwrap(), expected_authorization(case), "{name}: authorization");
}

#[test]
fn every_vector_the_signer_can_represent_signs_as_aws_does() {
    let mut ran = Vec::new();
    for case in cases() {
        let name = case.file_name().unwrap().to_string_lossy().into_owned();
        if SKIPPED.iter().all(|(skipped, _)| *skipped != name) {
            check(&case);
            ran.push(name);
        }
    }
    assert_eq!(ran.len(), 33, "{ran:?}");
}

#[test]
fn every_skip_names_a_vendored_case_and_says_why() {
    for (name, reason) in SKIPPED {
        assert!(suite().join(name).join("context.json").is_file(), "{name}");
        assert!(reason.len() > 20, "{name}");
    }
    assert_eq!(cases().len(), 33 + SKIPPED.len());
}

#[test]
fn the_suite_carries_its_licence_and_source_commit() {
    assert!(read(&suite(), "LICENSE").contains("Apache License"));
    assert!(read(&suite(), "NOTICE").contains("SPDX-License-Identifier: Apache-2.0"));
    assert!(read(&suite(), "SOURCE").contains("f556deacd6e6d9dbf540ff00a9bf383688076874"));
}
