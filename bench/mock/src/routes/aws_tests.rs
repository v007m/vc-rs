use axum::body::Bytes;
use axum::http::Request;
use axum::http::request::Parts;
use serde_json::json;

use super::{Refusal, verify};

/// Every AWS test-suite case's time, `2015-08-30T12:36:00Z`.
const SUITE_SECS: u64 = 1_440_938_160;
const SCOPE: &str = "Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request";

/// The suite's `get-vanilla-query-order-key-case`, signed with `signature`.
fn query_order(signature: &str) -> Parts {
    let authorization = format!("AWS4-HMAC-SHA256 {SCOPE}, SignedHeaders=host;x-amz-date, Signature={signature}");
    let request = Request::get("/?Param2=value2&Param1=value1")
        .header("host", "example.amazonaws.com")
        .header("x-amz-date", "20150830T123600Z")
        .header("authorization", authorization);
    request.body(()).unwrap().into_parts().0
}

const QUERY_ORDER: &str = "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500";

fn error(refusal: Result<serde_json::Value, Refusal>) -> serde_json::Value {
    refusal.unwrap_err().0
}

#[test]
fn accepts_the_suite_s_signatures() {
    let checked = verify(&query_order(QUERY_ORDER), &Bytes::new(), SUITE_SECS).unwrap();
    assert_eq!(checked, json!({"service": "service", "region": "us-east-1", "signedHeaders": ["host", "x-amz-date"]}));
    let authorization = format!(
        "AWS4-HMAC-SHA256 {SCOPE}, SignedHeaders=host;my-header1;x-amz-date, \
         Signature=c5410059b04c1ee005303aed430f6e6645f61f4dc9e1461ec8f8916fdf18852c"
    );
    let request = Request::post("/")
        .header("host", "example.amazonaws.com")
        .header("my-header1", "value1")
        .header("x-amz-date", "20150830T123600Z")
        .header("authorization", authorization);
    let parts = request.body(()).unwrap().into_parts().0;
    assert!(verify(&parts, &Bytes::new(), SUITE_SECS).is_ok(), "post-header-key-sort");
}

#[test]
fn a_mismatch_shows_the_canonical_request_the_server_computed() {
    let refused = error(verify(&query_order(&"0".repeat(64)), &Bytes::new(), SUITE_SECS));
    assert_eq!(refused["error"], "SignatureDoesNotMatch");
    let canonical = "GET\n/\nParam1=value1&Param2=value2\nhost:example.amazonaws.com\nx-amz-date:20150830T123600Z\n\n\
                     host;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(refused["canonicalRequest"], canonical);
    let mut tampered = query_order(QUERY_ORDER);
    tampered.headers.insert("host", "other.amazonaws.com".parse().unwrap());
    assert_eq!(error(verify(&tampered, &Bytes::new(), SUITE_SECS))["error"], "SignatureDoesNotMatch");
}

#[test]
fn a_date_more_than_five_minutes_off_is_too_skewed() {
    for now in [SUITE_SECS - 300, SUITE_SECS + 300] {
        assert!(verify(&query_order(QUERY_ORDER), &Bytes::new(), now).is_ok(), "{now}");
    }
    for now in [SUITE_SECS - 301, SUITE_SECS + 301] {
        assert_eq!(error(verify(&query_order(QUERY_ORDER), &Bytes::new(), now))["error"], "RequestTimeTooSkewed");
    }
}

#[test]
fn refuses_a_missing_malformed_or_unknown_signature() {
    let mut parts = query_order(QUERY_ORDER);
    parts.headers.remove("authorization");
    assert_eq!(error(verify(&parts, &Bytes::new(), SUITE_SECS))["error"], "MissingAuthenticationToken");
    parts.headers.insert("authorization", "Bearer t".parse().unwrap());
    assert_eq!(error(verify(&parts, &Bytes::new(), SUITE_SECS))["error"], "IncompleteSignature");
    let other = query_order(QUERY_ORDER).headers["authorization"].to_str().unwrap().replace("AKIDEXAMPLE", "AKIDOTHER");
    parts.headers.insert("authorization", other.parse().unwrap());
    assert_eq!(error(verify(&parts, &Bytes::new(), SUITE_SECS))["error"], "InvalidClientTokenId");
}

#[test]
fn a_body_hash_header_must_match_the_body() {
    let mut parts = query_order(QUERY_ORDER);
    parts.headers.insert("x-amz-content-sha256", "0".repeat(64).parse().unwrap());
    let refused = error(verify(&parts, &Bytes::from_static(b"x"), SUITE_SECS));
    assert_eq!(refused["error"], "XAmzContentSHA256Mismatch");
}

/// The path line of the canonical request the mock computes for `path`, signed for `service`.
fn canonical_path(path: &str, service: &str) -> String {
    let mut parts = query_order(&"0".repeat(64));
    parts.uri = path.parse().unwrap();
    let authorization = parts.headers["authorization"].to_str().unwrap().replace("/service/", &format!("/{service}/"));
    parts.headers.insert("authorization", authorization.parse().unwrap());
    let refused = error(verify(&parts, &Bytes::new(), SUITE_SECS));
    refused["canonicalRequest"].as_str().unwrap().lines().nth(1).unwrap().to_owned()
}

#[test]
fn normalises_and_encodes_again_every_path_but_s3_s() {
    let cases = [
        ("/./", "/"),
        ("/example/..", "/"),
        ("/./example", "/example"),
        ("/a//b/../c/", "/a/c/"),
        ("/a/.", "/a/"),
        ("/a%20b/c~d", "/a%2520b/c~d"),
    ];
    for (path, line) in cases {
        assert_eq!(canonical_path(path, "service"), line, "{path}");
    }
    for service in ["s3", "s3express", "s3-outposts", "s3-object-lambda"] {
        assert_eq!(canonical_path("/a//b/./c%20d", service), "/a//b/./c%20d", "{service}");
        assert_eq!(canonical_path("/a+b=c/d%2Fe", service), "/a%2Bb%3Dc/d%2Fe", "{service}");
    }
}

#[test]
fn reads_a_sigv4_date() {
    assert_eq!(super::seconds("20150830T123600Z"), Some(SUITE_SECS));
    assert_eq!(super::seconds("19700101T000000Z"), Some(0));
    assert_eq!(super::seconds("20240229T000001Z"), Some(1_709_164_801));
    assert_eq!(super::seconds("2015-08-30"), None);
    let bad = ["20151330T000000Z", "20150800T000000Z", "20150832T000000Z", "20150830T240000Z", "20150830T006000Z"];
    let more = ["20150830T0000000Z", "201508300T000000Z", "20150830T000061Z", "2015+830T12+600Z"];
    for stamp in bad.into_iter().chain(more).chain(["20150231T000000Z", "20150229T000000Z", "21000229T000000Z"]) {
        assert_eq!(super::seconds(stamp), None, "{stamp}");
    }
    assert_eq!(super::seconds("20151231T235960Z"), Some(1_451_606_400), "a leap second");
    assert_eq!(super::seconds("20000229T000000Z"), Some(951_782_400), "2000 is a leap year");
}
