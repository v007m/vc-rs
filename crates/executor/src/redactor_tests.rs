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
use crate::mask::MaskRules;

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
    let mut quoted = result(CheckKind::Header, "authorization", Some(json!("Bearer actual")));
    quoted.error = Some("got Bearer actual".to_owned());
    quoted.diff = Some("- Bearer x\n+ Bearer actual".to_owned());
    let check = redactor.check(quoted);
    assert_eq!((check.actual, check.diff, check.code), (Some(json!(MASKED)), None, Some("V5101".to_owned())));
    assert_eq!(check.error.as_deref(), Some(MASKED), "the error may quote the header");
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
    sink.publish(Event::CheckEvaluated { action, owner, check: Box::new(check) }).await;
    sink.publish(Event::ActionStarted { action, owner }).await;
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

fn leaky_check() -> CheckResult {
    let mut check = result(CheckKind::Equals, "body", Some(json!({"password": "pw-1", "ssn": "123-45-6789"})));
    check.expected = json!({"password": "pw-2", "key": "k=s3cr3t-key"});
    check.diff = Some("pw-1 pw-2".to_owned());
    check.error = Some("sent s3cr3t-key for 123-45-6789".to_owned());
    check
}

fn assert_masked(masked: CheckResult, leaks: &[&str]) {
    let text = format!("{masked:?}");
    for leak in leaks {
        assert!(!text.contains(leak), "{leak} in {text}");
    }
    assert_eq!(masked.expected["password"], json!(MASKED));
    assert!(masked.diff.is_some_and(|diff| diff.contains(MASKED)));
    assert!(masked.error.is_some_and(|error| error.contains(MASKED)));
}

#[test]
fn a_check_gets_the_built_in_masking_without_the_rules() {
    let masked = configured().check(leaky_check());
    assert!(format!("{masked:?}").contains("123-45-6789"));
    assert_masked(masked, &["pw-1", "pw-2", "s3cr3t-key"]);
}

#[tokio::test]
async fn a_check_event_gets_the_built_in_masking_and_the_rules() {
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let event = configured().event(Event::CheckEvaluated { action, owner, check: Box::new(leaky_check()) }).await;
    let Event::CheckEvaluated { check, .. } = event else { panic!("{event:?}") };
    assert_masked(*check, &["pw-1", "pw-2", "s3cr3t-key", "123-45-6789"]);
    let header = result(CheckKind::Header, "authorization", Some(json!("Bearer actual")));
    let event = configured().event(Event::CheckEvaluated { action, owner, check: Box::new(header) }).await;
    let Event::CheckEvaluated { check, .. } = event else { panic!("{event:?}") };
    assert_eq!((check.expected, check.actual), (json!(MASKED), Some(json!(MASKED))));
}

#[tokio::test]
async fn with_rules_keeps_the_masks_told_so_far_and_applies_only_the_new_rules() {
    let redactor = configured();
    let rules = MaskRules::standard()
        .for_config(&Config { redact_rules: vec![RedactRule::Regex("pw-\\d".to_owned())], ..Config::default() });
    let swapped = redactor.with_rules(rules);
    let failure = Failure::new("s3cr3t-key pw-1 123-45-6789");
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let event = Event::ActionFinished { action, owner, outcome: ActionOutcome::Failed(failure) };
    let Event::ActionFinished { outcome: ActionOutcome::Failed(masked), .. } = swapped.event(event).await else {
        panic!("not a failure")
    };
    assert_eq!(masked.message, format!("{MASKED} {MASKED} 123-45-6789"));
    assert_eq!(format!("{swapped:?}").matches("rules: 1").count(), 1);
}

#[tokio::test]
async fn a_masked_value_gets_the_built_in_masking_and_the_rules() {
    let leaky = json!({"password": "pw-1", "ssn": "123-45-6789", "key": "k=s3cr3t-key", "kept": "plain"});
    let masked = configured().masked(leaky.clone()).await;
    assert_eq!(masked, json!({"password": MASKED, "ssn": MASKED, "key": format!("k={MASKED}"), "kept": "plain"}));
    assert_eq!(configured().value(leaky)["ssn"], json!("123-45-6789"));
}

#[test]
fn a_check_without_a_diff_gets_none_after_masking() {
    let redactor = configured();
    let check = redactor.check(result(CheckKind::Equals, "body", Some(json!({"password": "pw-1"}))));
    assert_eq!((check.diff, check.actual), (None, Some(json!({"password": MASKED}))));
    assert_eq!(redactor.value(json!({"PASSWORD": "x"})), json!({"PASSWORD": MASKED}));
    assert!(!redactor.text("key s3cr3t-key").contains("s3cr3t-key"));
}

#[tokio::test]
async fn masks_failures_in_action_test_and_cleanup_events() {
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
        duration: std::time::Duration::ZERO,
    };
    let tested = Event::TestFinished { result };
    let cleanup = CleanupRecord { action, owner, outcome: ActionOutcome::Failed(failure()) };
    let cleaned = Event::CleanupFinished { cleanup };
    let warned = Event::Warned { action, owner, warning: failure() };
    for event in [finished, tested, cleaned, warned] {
        let text = format!("{:?}", redactor.event(event).await);
        assert!(!text.contains("s3cr3t-key") && text.contains(MASKED) && text.contains("V5101"), "{text}");
    }
    let passed = Event::ActionFinished { action, owner, outcome: ActionOutcome::Passed };
    assert_eq!(redactor.event(passed.clone()).await, passed);
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

#[tokio::test]
async fn masks_an_exchange_url_headers_and_both_bodies() {
    let redactor = configured();
    redactor.hide_values(["p@ss word"]);
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let uri = "https://api.test/k/s3cr3t-key?pw=p%40ss%20word";
    let sent = exchange(uri, r#"{"password":"pw-1","ssn":"123-45-6789"}"#);
    let Event::Exchanged { exchange: masked, .. } =
        redactor.event(Event::Exchanged { action, owner, exchange: Box::new(sent) }).await
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

#[tokio::test]
async fn an_exchange_and_a_check_masked_alone_are_masked_as_their_events_are() {
    let redactor = configured();
    let (action, owner) = (ActionId::new(0), OwnerKey::Run);
    let sent = exchange("https://api.test/k/s3cr3t-key", r#"{"password":"pw-1","ssn":"123-45-6789"}"#);
    let event = Event::Exchanged { action, owner, exchange: Box::new(sent.clone()) };
    let Event::Exchanged { exchange: as_event, .. } = redactor.event(event).await else { panic!("not an exchange") };
    assert_eq!(redactor.exchange(sent).await, *as_event);
    let event = Event::CheckEvaluated { action, owner, check: Box::new(leaky_check()) };
    let Event::CheckEvaluated { check: as_event, .. } = redactor.event(event).await else { panic!("not a check") };
    let alone = redactor.checked(leaky_check()).await;
    assert_eq!(alone, *as_event);
    assert_masked(alone, &["pw-1", "pw-2", "s3cr3t-key", "123-45-6789"]);
}

fn matched(segments: Vec<Segment>, actual: Value) -> CheckResult {
    let check = result(CheckKind::Match, "x", Some(actual));
    CheckResult { target: Some(Path { segments }), expected: json!("x"), error: Some("got hunter22".into()), ..check }
}

#[tokio::test]
async fn a_match_on_a_hidden_field_is_masked_as_it_is_in_the_body_and_so_is_its_failure() {
    let key = |name: &str| Segment::Key(name.to_owned());
    let hidden = [
        vec![key("password")],
        vec![key("profile"), key("PASSWORD")],
        vec![key("items"), Segment::All, key("password")],
        vec![key("password"), Segment::Index(0)],
    ];
    for segments in hidden {
        let check = matched(segments.clone(), json!("hunter22"));
        let redactor = configured();
        let event = redactor.event(Event::CheckEvaluated {
            action: ActionId::new(0),
            owner: OwnerKey::Run,
            check: Box::new(check.clone()),
        });
        let Event::CheckEvaluated { check: evented, .. } = event.await else { panic!("not a check") };
        for masked in [configured().check(check), *evented] {
            assert_eq!((&masked.expected, &masked.actual), (&json!(MASKED), &Some(json!(MASKED))), "{segments:?}");
            assert_eq!(masked.error.as_deref(), Some(MASKED));
            let failure = crate::check::verdict(&[masked]).unwrap_or_else(|| Failure::new("passed"));
            assert!(!failure.message.contains("hunter22"), "{}", failure.message);
        }
    }
    let shown = configured().check(matched(vec![key("name")], json!("hunter22")));
    assert_eq!((shown.actual, shown.error.as_deref()), (Some(json!("hunter22")), Some("got hunter22")));
}

#[tokio::test]
async fn a_failed_check_on_a_map_holding_a_hidden_field_has_its_error_masked_whole() {
    let profile = vec![Segment::Key("profile".to_owned())];
    let mut check = matched(profile, json!({"password": "hunter22", "name": "ada"}));
    check.error = Some(r#"expected text, got {"password":"hunter22","name":"ada"}"#.to_owned());
    let redactor = configured();
    for masked in [redactor.check(check.clone()), redactor.checked(check).await] {
        assert_eq!(masked.actual, Some(json!({"password": MASKED, "name": "ada"})));
        assert_eq!(masked.error.as_deref(), Some(MASKED), "the error quotes the map");
    }
    let mut plain = matched(vec![Segment::Key("profile".to_owned())], json!({"name": "ada"}));
    plain.error = Some("got ada".to_owned());
    assert_eq!(redactor.checked(plain).await.error.as_deref(), Some("got ada"), "nothing hidden, error kept");
}

#[tokio::test]
async fn hides_header_and_masked_text_follow_the_masks_and_the_rules() {
    let redactor = configured();
    redactor.hide_headers(["X-Session"]);
    assert_eq!(["AUTHORIZATION", "x-session", "x-trace"].map(|name| redactor.hides_header(name)), [true, true, false]);
    let text = redactor.masked_text("key s3cr3t-key for 123-45-6789, id 7").await;
    assert_eq!(text, format!("key {MASKED} for {MASKED}, id 7"));
}

#[tokio::test]
async fn only_a_match_check_reads_the_body_and_a_shown_field_in_a_list_keeps_its_value() {
    let key = |name: &str| Segment::Key(name.to_owned());
    let redactor = configured();
    let listed = redactor.checked(matched(vec![key("items"), Segment::All, key("name")], json!("ada"))).await;
    assert_eq!((listed.expected, listed.actual), (json!("x"), Some(json!("ada"))));
    let header = CheckResult { kind: CheckKind::Header, ..matched(vec![key("password")], json!("ada")) };
    let header = redactor.checked(header).await;
    assert_eq!((header.expected, header.actual), (json!("x"), Some(json!("ada"))), "a header is not a body field");
}
