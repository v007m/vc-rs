use std::sync::Arc;

use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_TYPE, SET_COOKIE};
use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use serde_json::{Value, json};
use vc_model::{ConnectionInfo, Exchange, HttpResponse, RedactRule, RenderedRequest, Timing};

use super::{MASKED_IN_URL, exchange};
use crate::MASKED;
use crate::mask::{MaskRule, Masking, Masks, rule};

const SECRET: &str = "s3cr3t-key-4242";

fn masks() -> Masks {
    let values = [SECRET.to_owned(), crate::action::encode("p@ss word!")];
    Masks::default().with_values(&values).with_headers(&["authorization".to_owned(), "set-cookie".to_owned()])
}

fn sent(uri: &str, headers: HeaderMap, body: &[u8]) -> Exchange {
    let request = RenderedRequest {
        method: Method::POST,
        uri: uri.parse().unwrap(),
        headers,
        body: Bytes::copy_from_slice(body),
    };
    let response = HttpResponse { status: StatusCode::OK, headers: HeaderMap::new(), body: Bytes::new(), wire_size: 0 };
    Exchange { request, response, timing: Timing::default(), connection: ConnectionInfo::default() }
}

fn masked(exchange_in: Exchange) -> Exchange {
    masked_with(&[], exchange_in)
}

fn masked_with(rules: &[Arc<dyn MaskRule>], exchange_in: Exchange) -> Exchange {
    let masks = masks();
    exchange(&Masking { masks: &masks, rules }, exchange_in)
}

fn header(value: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_str(value).unwrap());
    headers
}

#[test]
fn a_secret_in_the_path_or_the_query_is_masked_and_the_url_still_parses() {
    let uri = format!("https://api.test/keys/{SECRET}?key={SECRET}&pw=p%40ss%20word%21&page=2");
    let shown = masked(sent(&uri, HeaderMap::new(), b"")).request.uri;
    let expected = format!("https://api.test/keys/{MASKED_IN_URL}?key={MASKED_IN_URL}&pw={MASKED_IN_URL}&page=2");
    assert_eq!(shown, expected.parse::<Uri>().unwrap());
    let clean = "https://api.test/users?page=2";
    assert_eq!(masked(sent(clean, HeaderMap::new(), b"")).request.uri, clean.parse::<Uri>().unwrap());
}

#[test]
fn a_rule_that_breaks_the_url_leaves_only_its_scheme_and_host() {
    let broken: Vec<Arc<dyn MaskRule>> = vec![Arc::new(Spaces)];
    let shown = masked_with(&broken, sent("https://api.test/users/7", HeaderMap::new(), b"")).request.uri;
    assert_eq!(shown, format!("https://api.test/{MASKED_IN_URL}").parse::<Uri>().unwrap());
}

/// A rule that turns text into something no URL can hold.
struct Spaces;

impl MaskRule for Spaces {
    fn text(&self, text: &str) -> Option<String> {
        Some(text.replace('/', " /"))
    }

    fn field(&self, _: &str, _: &Value) -> bool {
        false
    }
}

#[test]
fn masked_headers_are_replaced_whole_and_others_lose_their_secrets() {
    let mut headers = header(&format!("key {SECRET}"));
    headers.insert(AUTHORIZATION, HeaderValue::from_static("Basic YWRhOmh1bnRlcg=="));
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let mut response = sent("https://api.test/", headers, b"");
    response.response.headers.insert(SET_COOKIE, HeaderValue::from_static("sid=abc123; HttpOnly"));
    let shown = masked(response);
    let request = &shown.request.headers;
    assert_eq!(request[AUTHORIZATION], MASKED);
    assert_eq!(request["x-api-key"], format!("key {MASKED}").as_str());
    assert_eq!(request[CONTENT_TYPE], "application/json");
    assert!(request[CONTENT_TYPE].is_sensitive());
    assert_eq!(shown.response.headers[SET_COOKIE], MASKED);
}

#[test]
fn a_header_that_is_not_text_is_replaced() {
    let headers = {
        let mut headers = HeaderMap::new();
        headers.insert("x-raw", HeaderValue::from_bytes(b"caf\xe9").unwrap());
        headers
    };
    assert_eq!(masked(sent("https://api.test/", headers, b"")).request.headers["x-raw"], MASKED);
}

#[test]
fn bodies_lose_secrets_and_hidden_fields_in_both_directions() {
    let mut exchange_in = sent("https://api.test/", HeaderMap::new(), format!("token={SECRET}").as_bytes());
    let body = json!({"note": format!("is {SECRET}"), "id": 7});
    exchange_in.response.body = Bytes::from(body.to_string());
    let shown = masked(exchange_in);
    assert_eq!(shown.request.body, Bytes::from(format!("token={MASKED}")));
    let response: Value = serde_json::from_slice(&shown.response.body).unwrap();
    assert_eq!(response, json!({"note": format!("is {MASKED}"), "id": 7}));
}

#[test]
fn a_secret_in_a_json_key_is_masked_and_a_clean_body_keeps_its_bytes() {
    let keyed = format!("{{\"{SECRET}\": 1}}");
    let shown = masked(sent("https://api.test/", HeaderMap::new(), keyed.as_bytes()));
    assert_eq!(shown.request.body, Bytes::from(format!("{{\"{MASKED}\": 1}}")));
    let spaced = b"{ \"id\" :  7 }";
    assert_eq!(masked(sent("https://api.test/", HeaderMap::new(), spaced)).request.body, &spaced[..]);
}

#[test]
fn json_fields_a_rule_hides_are_masked() {
    let rules = vec![rule(&RedactRule::Regex(r"\d{3}-\d{2}-\d{4}".to_owned()))];
    let body = json!({"ssn": "123-45-6789"}).to_string();
    let shown = masked_with(&rules, sent("https://api.test/", HeaderMap::new(), body.as_bytes()));
    assert_eq!(serde_json::from_slice::<Value>(&shown.request.body).unwrap(), json!({"ssn": MASKED}));
}

#[test]
fn bytes_that_are_not_text_are_replaced_only_when_they_hold_a_secret() {
    let mut leaking = vec![0xff, 0xfe];
    leaking.extend_from_slice(SECRET.as_bytes());
    let shown = masked(sent("https://api.test/", HeaderMap::new(), &leaking));
    assert_eq!(shown.request.body, MASKED.as_bytes());
    let clean = [0xff, 0xfe, 0x00, 0x01];
    assert_eq!(masked(sent("https://api.test/", HeaderMap::new(), &clean)).request.body, &clean[..]);
}
