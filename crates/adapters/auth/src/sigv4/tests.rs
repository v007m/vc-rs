use http::header::{AUTHORIZATION, CONTENT_LENGTH, RANGE, USER_AGENT};
use http::{HeaderValue, Method, Uri};
use serde_json::{Value, json};
use vc_interfaces::{AuthError, RequestSigner as _};
use vc_model::RenderedRequest;

use super::crypto::{Moment, hex, hmac};
use super::vectors::SUITE_MS as NOW;
use super::{CONTENT, DATE, SigV4, TOKEN, canonical};
use crate::testing::request;

const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

fn settings(extra: &Value) -> Value {
    let mut settings = json!({
        "region": "us-east-1",
        "service": "service",
        "accessKeyId": "AKIDEXAMPLE",
        "secretAccessKey": SECRET,
    });
    for (key, value) in extra.as_object().into_iter().flatten() {
        settings[key] = value.clone();
    }
    settings
}

fn signer(extra: &Value) -> SigV4 {
    SigV4::from_request(&request("awsSigV4", &settings(extra))).unwrap()
}

fn get(uri: &str) -> RenderedRequest {
    RenderedRequest::new(Method::GET, Uri::try_from(uri).unwrap())
}

fn missing(extra: &Value) -> String {
    match SigV4::from_request(&request("awsSigV4", &settings(extra))) {
        Err(AuthError::Missing { auth, key }) => {
            assert_eq!(auth, "api");
            key
        }
        other => panic!("{other:?}"),
    }
}

fn canonical_text(signer: &SigV4, request: &RenderedRequest) -> String {
    let payload = signer.payload_hash(&request.body);
    String::from_utf8_lossy(canonical::request(request, &payload, signer.s3).text()).into_owned()
}

#[test]
fn a_signed_request_carries_the_date_the_body_hash_and_the_authorization() {
    let signed = signer(&json!({})).sign(get("https://example.amazonaws.com/"), NOW).unwrap();
    assert_eq!(signed.headers[DATE], "20150830T123600Z");
    assert_eq!(signed.headers[CONTENT], "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    let authorization = signed.headers[AUTHORIZATION].to_str().unwrap();
    assert!(authorization.starts_with(
        "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, \
         SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="
    ));
    assert_eq!(authorization.rsplit_once('=').unwrap().1.len(), 64);
    assert!(signed.headers[AUTHORIZATION].is_sensitive());
    assert!(!signed.headers.contains_key(TOKEN));
}

/// AWS's published S3 "GET Object" example, signed end to end through `sign`.
#[test]
fn sign_reproduces_the_aws_s3_get_object_example() {
    let settings = json!({
        "region": "us-east-1",
        "service": "s3",
        "accessKeyId": "AKIAIOSFODNN7EXAMPLE",
        "secretAccessKey": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    });
    let signer = SigV4::from_request(&request("awsSigV4", &settings)).unwrap();
    let mut object = get("https://examplebucket.s3.amazonaws.com/test.txt");
    object.headers.insert(RANGE, HeaderValue::from_static("bytes=0-9"));
    let sent = signer.sign(object, 1_369_353_600_000).unwrap();
    assert_eq!(
        sent.headers[AUTHORIZATION],
        "AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, \
         SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, \
         Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
    );
}

#[test]
fn signing_replaces_an_authorization_and_a_date_the_request_already_has() {
    let mut request = get("https://example.amazonaws.com/");
    request.headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer old"));
    request.headers.append(DATE, HeaderValue::from_static("19990101T000000Z"));
    request.headers.append(DATE, HeaderValue::from_static("20000101T000000Z"));
    let signed = signer(&json!({})).sign(request, NOW).unwrap();
    assert_eq!(signed.headers.get_all(DATE).iter().collect::<Vec<_>>(), ["20150830T123600Z"]);
    assert_eq!(signed.headers.get_all(AUTHORIZATION).iter().count(), 1);
    assert!(signed.headers[AUTHORIZATION].to_str().unwrap().starts_with("AWS4-HMAC-SHA256 "));
}

#[test]
fn a_session_token_is_sent_signed_and_marked_sensitive() {
    let signed = signer(&json!({"sessionToken": "IQoJb3JpZ2luX2VjE"})).sign(get("https://e.example/"), NOW).unwrap();
    assert_eq!(signed.headers[TOKEN], "IQoJb3JpZ2luX2VjE");
    assert!(signed.headers[TOKEN].is_sensitive());
    assert!(signed.headers[AUTHORIZATION].to_str().unwrap().contains("x-amz-date;x-amz-security-token,"));
}

#[test]
fn an_empty_or_null_session_token_sends_no_token() {
    for token in [json!(""), Value::Null] {
        let signed = signer(&json!({"sessionToken": token})).sign(get("https://e.example/"), NOW).unwrap();
        assert!(!signed.headers.contains_key(TOKEN));
    }
}

#[test]
fn an_unsigned_payload_signs_the_literal_and_not_the_body() {
    let mut request = RenderedRequest::new(Method::PUT, Uri::from_static("https://e.example/o"));
    request.body = "hello".into();
    let unsigned = signer(&json!({"payload": "unsigned"}));
    let sent = unsigned.sign(request, NOW).unwrap();
    assert_eq!(sent.headers[CONTENT], "UNSIGNED-PAYLOAD");
    assert!(canonical_text(&unsigned, &sent).ends_with("\nUNSIGNED-PAYLOAD"));
}

#[test]
fn the_body_hash_is_the_last_line_of_the_canonical_request() {
    let mut request = RenderedRequest::new(Method::POST, Uri::from_static("https://e.example/"));
    request.body = "hello".into();
    let text = canonical_text(&signer(&json!({"payload": "signed"})), &request);
    assert!(text.ends_with("\n2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"));
}

#[test]
fn transport_headers_are_left_unsigned_and_host_comes_from_the_url() {
    let mut request = get("https://Example.COM:8443/");
    request.headers.insert(USER_AGENT, HeaderValue::from_static("vc"));
    request.headers.insert(CONTENT_LENGTH, HeaderValue::from_static("0"));
    request.headers.insert("x-amzn-trace-id", HeaderValue::from_static("Root=1"));
    request.headers.insert("x-custom", HeaderValue::from_static("  a \t  b  "));
    request.headers.append("x-custom", HeaderValue::from_static("c"));
    let text = canonical_text(&signer(&json!({})), &request);
    assert_eq!(text, format!("GET\n/\n\nhost:example.com:8443\nx-custom:a b,c\n\nhost;x-custom\n{}", empty_hash()));
}

#[test]
fn a_host_header_the_request_sets_is_the_one_signed() {
    let mut request = get("https://e.example/");
    request.headers.insert("host", HeaderValue::from_static("other.example"));
    assert!(canonical_text(&signer(&json!({})), &request).contains("\nhost:other.example\n"));
}

#[test]
fn a_header_that_is_not_utf8_is_signed_as_its_bytes() {
    let mut request = get("https://e.example/");
    request.headers.insert("x-raw", HeaderValue::from_bytes(b"caf\xe9").unwrap());
    let signer = signer(&json!({}));
    let payload = signer.payload_hash(&request.body);
    let canonical = canonical::request(&request, &payload, false);
    assert!(canonical.text().windows(11).any(|bytes| bytes == b"x-raw:caf\xe9\n"));
    assert!(signer.sign(request, NOW).is_ok());
}

#[test]
fn a_path_is_normalised_and_encoded_twice_for_most_services() {
    let text = canonical_text(&signer(&json!({})), &get("https://e.example/a/./b/../c%20d//"));
    assert!(text.starts_with("GET\n/a/c%2520d/\n"), "{text}");
    let text = canonical_text(&signer(&json!({})), &get("https://e.example/a/.."));
    assert!(text.starts_with("GET\n/\n"), "{text}");
}

#[test]
fn an_s3_path_is_encoded_once_and_sent_as_signed() {
    for service in ["s3", "s3express", "s3-outposts", "s3-object-lambda"] {
        let s3 = signer(&json!({"service": service}));
        let sent = s3.sign(get("https://b.s3.amazonaws.com/a%20b/./c+d/%zz?x=1"), NOW).unwrap();
        assert_eq!(sent.uri.path(), "/a%20b/./c%2Bd/%25zz", "{service}");
        assert_eq!(sent.uri.query(), Some("x=1"));
        assert!(canonical_text(&s3, &sent).starts_with("GET\n/a%20b/./c%2Bd/%25zz\nx=1\n"), "{service}");
        assert_eq!(canonical::sent_uri(sent.uri.clone(), true), sent.uri);
    }
}

#[test]
fn a_service_named_like_s3_but_not_s3_normalises_its_path() {
    let sent = signer(&json!({"service": "s3x"})).sign(get("https://e.example/a/./b+c"), NOW).unwrap();
    assert_eq!(sent.uri.path(), "/a/./b+c");
    assert!(canonical_text(&signer(&json!({"service": "s3x"})), &sent).starts_with("GET\n/a/b%2Bc\n"));
}

#[test]
fn the_query_is_decoded_encoded_and_sorted() {
    let text = canonical_text(&signer(&json!({})), &get("https://e.example/?b=2&a=1&&a=0&c&d=x%2By%2a"));
    assert!(text.starts_with("GET\n/\na=0&a=1&b=2&c=&d=x%2By%2A\n"), "{text}");
}

#[test]
fn a_plus_in_the_query_is_sent_and_signed_as_a_space() {
    let signer = signer(&json!({}));
    let sent = signer.sign(get("https://e.example/a+b?q=x+y&n=1%2B1"), NOW).unwrap();
    assert_eq!(sent.uri.path(), "/a+b");
    assert_eq!(sent.uri.query(), Some("q=x%20y&n=1%2B1"));
    assert!(canonical_text(&signer, &sent).starts_with("GET\n/a%2Bb\nn=1%2B1&q=x%20y\n"));
    assert!(canonical_text(&signer, &get("https://e.example/?q=x+y")).contains("\nq=x%20y\n"));
    let plain = get("https://e.example/?q=x");
    let uri = plain.uri.clone();
    assert_eq!(signer.sign(plain, NOW).unwrap().uri, uri);
}

#[test]
fn each_bad_setting_names_its_key_and_never_its_value() {
    const KEY_ID: &str = "accessKeyId: printable ASCII with no spaces, /, comma or =";
    const TOKEN_RULE: &str = "sessionToken: a header value, with no line breaks";
    let cases = [
        (json!({"accessKeyId": ""}), "accessKeyId"),
        (json!({"secretAccessKey": ""}), "secretAccessKey"),
        (json!({"accessKeyId": "AKID/EXAMPLE"}), KEY_ID),
        (json!({"accessKeyId": "AKID EXAMPLE"}), KEY_ID),
        (json!({"accessKeyId": "AKID,X"}), KEY_ID),
        (json!({"accessKeyId": "AKID=X"}), KEY_ID),
        (json!({"region": "US-East-1"}), "region: lowercase letters, digits and -"),
        (json!({"service": "s3/x"}), "service: lowercase letters, digits and -"),
        (json!({"sessionToken": "a\nb"}), TOKEN_RULE),
        (json!({"sessionToken": 12}), TOKEN_RULE),
        (json!({"payload": "streaming"}), "payload: signed or unsigned"),
        (json!({"payload": false}), "payload: signed or unsigned"),
    ];
    for (extra, key) in cases {
        assert_eq!(missing(&extra), key, "{extra}");
    }
}

#[test]
fn a_missing_setting_is_missing() {
    let mut settings = settings(&json!({}));
    settings.as_object_mut().unwrap().remove("region");
    let error = SigV4::from_request(&request("awsSigV4", &settings)).unwrap_err();
    assert!(matches!(error, AuthError::Missing { ref key, .. } if key == "region"), "{error:?}");
}

#[test]
fn passes_the_request_signer_suite() {
    let mut request = get("https://example.amazonaws.com/?Param1=value1");
    request.headers.insert("x-trace", HeaderValue::from_static("t1"));
    vc_testkit::request_signer_suite(&signer(&json!({"sessionToken": "t0k3n-session"})), &request, NOW);
    vc_testkit::request_signer_suite(
        &signer(&json!({"service": "s3"})),
        &get("https://bucket.s3.amazonaws.com/a+b/c%2Fd"),
        NOW,
    );
}

#[test]
fn debug_shows_the_region_and_service_and_no_credential() {
    let shown = format!("{:?}", signer(&json!({"sessionToken": "t0k3n-session"})));
    assert_eq!(shown, r#"SigV4 { region: "us-east-1", service: "service", .. }"#);
    assert!(!shown.contains(SECRET) && !shown.contains("t0k3n") && !shown.contains("AKID"));
}

#[test]
fn the_time_is_utc_and_out_of_range_reads_as_the_epoch() {
    assert_eq!(Moment::at(NOW).stamp, "20150830T123600Z");
    assert_eq!(Moment::at(NOW + 41_039_999).stamp, "20150830T235959Z");
    let epoch = Moment::at(u64::MAX);
    assert_eq!((epoch.date.as_str(), epoch.stamp.as_str()), ("19700101", "19700101T000000Z"));
}

#[test]
fn hmac_and_hex_match_rfc_4231() {
    let mac = hmac(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(hex(&*mac), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
}

fn empty_hash() -> &'static str {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
