use vc_model::Bodies;

use super::*;
use crate::testing::exchange as sample;

#[test]
fn keeps_short_bodies_whole_as_text() {
    let view = view(&sample("{\"a\":1}", "ok"), 64);
    assert_eq!((view.method.as_str(), view.url.as_str(), view.status), ("POST", "http://localhost/users", 201));
    assert_eq!(view.request_body.as_deref(), Some("{\"a\":1}"));
    assert_eq!(view.response_body.as_deref(), Some("ok"));
    assert!(!view.truncated);
    assert_eq!(view.request_headers, [("authorization".to_owned(), "Bearer ***".to_owned())]);
    assert_eq!(view.response_headers, [("x-raw".to_owned(), "caf\u{fffd}".to_owned())], "lossy text");
    assert_eq!((view.timing.dns, view.timing.connect, view.timing.total), (Some(1.5), None, 12.0));
    assert!(view.action.is_empty() && view.api.is_none(), "the caller names them");
}

#[test]
fn cuts_long_bodies_on_a_character_boundary() {
    let view = view(&sample("ééé", ""), 3);
    assert_eq!(view.request_body.as_deref(), Some("é"), "3 bytes end inside the second é");
    assert_eq!(view.response_body, None, "an empty body is no body");
    assert!(view.truncated);
    let stripped = without_bodies(view);
    assert_eq!((stripped.request_body, stripped.truncated), (None, false));
}

#[test]
fn keeps_bodies_as_history_bodies_says() {
    let keeping = |bodies| Keeping { bodies, max_body: 1 };
    assert!(!keeping(Bodies::None).bodies_of(true));
    assert!(keeping(Bodies::Failures).bodies_of(true));
    assert!(!keeping(Bodies::Failures).bodies_of(false));
    assert!(keeping(Bodies::All).bodies_of(false));
}
