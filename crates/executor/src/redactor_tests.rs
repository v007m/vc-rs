use std::sync::Arc;

use bytes::Bytes;
use http::header::AUTHORIZATION;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use vc_model::{
    ActionId, ActionOutcome, CheckKind, CheckResult, CleanupRecord, Config, ConnectionInfo, Event, Exchange, Failure,
    Feed, HttpResponse, Origin, OwnerKey, Path, Pos, RedactRule, RenderedRequest, Segment, SourceFileId, SourceSpan,
    TestId, TestResult, TestStatus, Timing,
};

use super::{MASKED, RedactingSink, Redactor};
use crate::EventBus;

fn result(kind: CheckKind, header: &str, actual: Option<Value>) -> CheckResult {
    let span = SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0));
    CheckResult {
        kind,
        target: Some(Path { segments: vec![Segment::Key(header.to_owned())] }),
        origin: Origin::Action(ActionId::new(0)),
        span,
        passed: false,
        expected: json!("Bearer expected"),
        actual,
        diff: None,
        error: None,
        code: Some("V5101".to_owned()),
    }
}

fn masked(redactor: &Redactor, kind: CheckKind, header: &str) -> bool {
    let check = redactor.check(result(kind, header, Some(json!("Bearer actual"))));
    check.expected == json!(MASKED)
}

#[test]
fn masks_the_always_masked_and_configured_headers_in_any_case() {
    let redactor = Redactor::new(&["X-Api-Key".to_owned()]);
    for header in ["Authorization", "cookie", "SET-COOKIE", "x-api-key"] {
        assert!(masked(&redactor, CheckKind::Header, header), "{header}");
    }
    assert!(!masked(&redactor, CheckKind::Header, "content-type"));
    assert!(!masked(&redactor, CheckKind::Match, "authorization"));
    let check = redactor.check(result(CheckKind::Header, "authorization", Some(json!("Bearer actual"))));
    assert_eq!((check.actual, check.diff, check.code), (Some(json!(MASKED)), None, Some("V5101".to_owned())));
    let missing = redactor.check(result(CheckKind::Header, "authorization", None));
    assert_eq!(missing.actual, None);
}

#[test]
fn leaves_a_check_without_a_single_header_target_alone() {
    let redactor = Redactor::new(&[]);
    let mut nested = result(CheckKind::Header, "authorization", Some(json!("x")));
    nested.target = Some(Path { segments: vec![Segment::Key("authorization".to_owned()), Segment::Index(0)] });
    assert_eq!(redactor.check(nested.clone()), nested);
    let mut whole = nested;
    whole.target = None;
    assert_eq!(redactor.check(whole.clone()), whole);
}

#[test]
fn headers_hidden_later_are_masked_from_then_on() {
    let redactor = Redactor::new(&[]);
    assert!(!masked(&redactor, CheckKind::Header, "x-session"));
    redactor.hide_headers(["X-Session"]);
    assert!(masked(&redactor, CheckKind::Header, "x-session"));
    assert!(masked(&redactor, CheckKind::Header, "authorization"));
}

#[tokio::test]
async fn the_sink_masks_check_events_and_passes_the_rest() {
    let bus = EventBus::spawn();
    let mut feed = bus.subscribe(8);
    let sink = RedactingSink::new(bus, Arc::new(Redactor::new(&[])));
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let check = result(CheckKind::Header, "authorization", Some(json!("Bearer actual")));
    sink.publish(Event::CheckEvaluated { action, owner, check: Box::new(check) });
    sink.publish(Event::ActionStarted { action, owner });
    sink.flush().await;
    let Some(Feed::Event(first)) = feed.recv().await else { panic!("no event") };
    let Event::CheckEvaluated { check, .. } = first.event else { panic!("{first:?}") };
    assert_eq!((check.expected, check.actual), (json!(MASKED), Some(json!(MASKED))));
    let Some(Feed::Event(second)) = feed.recv().await else { panic!("no event") };
    assert_eq!(second.event, Event::ActionStarted { action, owner });
}

fn configured() -> Redactor {
    let config = Config {
        redact_fields: vec!["Password".to_owned()],
        redact_rules: vec![RedactRule::Regex(r"\d{3}-\d{2}-\d{4}".to_owned())],
        ..Config::default()
    };
    let redactor = Redactor::for_config(&config);
    redactor.hide_values(["s3cr3t-key"]);
    redactor.hide_values(Vec::<String>::new());
    redactor
}

#[test]
fn masks_secret_values_hidden_fields_and_rule_matches_in_a_check() {
    let redactor = configured();
    let mut check = result(CheckKind::Equals, "body", Some(json!({"password": "pw-1", "ssn": "123-45-6789"})));
    check.expected = json!({"password": "pw-2", "key": "k=s3cr3t-key"});
    check.diff = Some("pw-1 pw-2".to_owned());
    check.error = Some("sent s3cr3t-key for 123-45-6789".to_owned());
    let masked = redactor.check(check);
    let text = format!("{masked:?}");
    for leak in ["pw-1", "pw-2", "s3cr3t-key", "123-45-6789"] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
    assert_eq!(masked.expected["password"], json!(MASKED));
    assert!(masked.diff.is_some_and(|diff| diff.contains(MASKED)));
    assert!(masked.error.is_some_and(|error| error.contains(MASKED)));
}

#[test]
fn a_check_without_a_diff_gets_none_after_masking() {
    let redactor = configured();
    let check = redactor.check(result(CheckKind::Equals, "body", Some(json!({"password": "pw-1"}))));
    assert_eq!((check.diff, check.actual), (None, Some(json!({"password": MASKED}))));
    assert_eq!(redactor.value(json!({"PASSWORD": "x"})), json!({"PASSWORD": MASKED}));
    assert!(!redactor.text("key s3cr3t-key").contains("s3cr3t-key"));
}

#[test]
fn masks_failures_in_action_test_and_cleanup_events() {
    let redactor = configured();
    let failure = || Failure { code: Some("V5101".to_owned()), ..Failure::new("got s3cr3t-key") };
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let finished = Event::ActionFinished { action, owner, outcome: ActionOutcome::Failed(failure()) };
    let result = TestResult {
        test: TestId::new(vc_model::StringTable::new().intern("t").unwrap()),
        instance: None,
        status: TestStatus::Failed,
        attempts: 1,
        failure: Some(failure()),
        skip_reason: Some("needs s3cr3t-key".to_owned()),
    };
    let tested = Event::TestFinished { result };
    let cleanup = CleanupRecord { action, owner, outcome: ActionOutcome::Failed(failure()) };
    let cleaned = Event::CleanupFinished { cleanup };
    let warned = Event::Warned { action, owner, warning: failure() };
    for event in [finished, tested, cleaned, warned] {
        let text = format!("{:?}", redactor.event(event));
        assert!(!text.contains("s3cr3t-key") && text.contains(MASKED) && text.contains("V5101"), "{text}");
    }
    let passed = Event::ActionFinished { action, owner, outcome: ActionOutcome::Passed };
    assert_eq!(redactor.event(passed.clone()), passed);
}

fn exchange(uri: &str, body: &str) -> Exchange {
    let mut request = RenderedRequest::new(http::Method::POST, uri.parse().unwrap());
    request.headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer tok-abcdef"));
    request.body = Bytes::from(body.to_owned());
    let mut headers = HeaderMap::new();
    headers.insert("x-echo", HeaderValue::from_static("key s3cr3t-key for 123-45-6789"));
    let response = HttpResponse { status: StatusCode::OK, headers, body: Bytes::from(body.to_owned()), wire_size: 0 };
    Exchange { request, response, timing: Timing::default(), connection: ConnectionInfo::default() }
}

fn shown(headers: &HeaderMap) -> String {
    headers.values().map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned()).collect::<Vec<_>>().join(" ")
}

#[test]
fn masks_an_exchange_url_headers_and_both_bodies() {
    let redactor = configured();
    redactor.hide_values(["p@ss word"]);
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let uri = "https://api.test/k/s3cr3t-key?pw=p%40ss%20word";
    let sent = exchange(uri, r#"{"password":"pw-1","ssn":"123-45-6789"}"#);
    let Event::Exchanged { exchange: masked, .. } =
        redactor.event(Event::Exchanged { action, owner, exchange: Box::new(sent) })
    else {
        panic!("not an exchange")
    };
    let (request, response) = (&masked.request, &masked.response);
    let bodies = format!("{:?} {:?}", request.body, response.body);
    let text = format!("{} {bodies} {} {}", request.uri, shown(&request.headers), shown(&response.headers));
    for leak in ["s3cr3t-key", "p%40ss", "pw-1", "123-45-6789", "tok-abcdef"] {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
    assert_eq!((request.headers[AUTHORIZATION].as_bytes(), response.status), (MASKED.as_bytes(), StatusCode::OK));
}
