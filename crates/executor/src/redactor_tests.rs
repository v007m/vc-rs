use std::sync::Arc;

use serde_json::{Value, json};
use vc_model::{
    ActionId, CheckKind, CheckResult, Event, Feed, Origin, OwnerKey, Path, Pos, Segment, SourceFileId, SourceSpan,
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
        diff: Some("-a\n+b".to_owned()),
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
