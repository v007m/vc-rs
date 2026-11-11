#![cfg(test)]

use std::time::Duration;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{
    Namespace, Pos, Reference, ScalarSource, ScalarStyle, SourceFileId, SourceSpan, StatusPattern, StringTable,
};
use vc_template::Roots;

fn span() -> SourceSpan {
    SourceSpan::new(SourceFileId::new(0), Pos::new(4, 9, 50), Pos::new(4, 14, 55))
}

fn reference(text: &str, strings: &mut StringTable) -> Reference {
    let end = Pos::new(1, 1 + u32::try_from(text.len()).unwrap(), text.len());
    let span = SourceSpan::new(SourceFileId::new(0), Pos::START, end);
    vc_template::parse(text, ScalarSource { span, style: ScalarStyle::Plain }, strings)
        .unwrap()
        .single_ref()
        .cloned()
        .unwrap()
}

fn check(roots: &Roots, text: &str) -> Result<(), Box<Diagnostic>> {
    let mut strings = StringTable::new();
    roots.check(&reference(text, &mut strings), &strings)
}

fn code(roots: &Roots, text: &str) -> Code {
    check(roots, text).unwrap_err().code
}

#[test]
fn everyday_roots_are_available_everywhere() {
    for text in ["${var.a}", "${env.HOME}", "${secret.key}", "${fake.uuid}", "${dataset.users}", "${run.id}"] {
        assert!(check(&Roots::new(), text).is_ok(), "{text}");
    }
}

#[test]
fn only_keeps_the_named_roots_whatever_else_is_added() {
    let why = "read before the run";
    let roots = Roots::new().with_call().in_for_each().only(&[Namespace::Var, Namespace::Dataset], why);
    assert!(check(&roots, "${var.a}").is_ok() && check(&roots, "${dataset.users | take(3)}").is_ok());
    for (text, name) in
        [("${fake.uuid}", "fake"), ("${response.body}", "response"), ("${item.n}", "item"), ("${seed.id}", "seed")]
    {
        let diagnostic = *check(&roots, text).unwrap_err();
        assert_eq!((diagnostic.code, diagnostic.message), (Code::V2105, format!("`{name}` is not available here")));
        assert_eq!(diagnostic.help, [why]);
    }
}

#[test]
fn call_roots_need_a_call() {
    let diagnostic = *check(&Roots::new(), "${response.body}").unwrap_err();
    assert_eq!((diagnostic.code, diagnostic.message.as_str()), (Code::V2105, "`response` is not available here"));
    assert_eq!(diagnostic.help, ["`response` is available once a call is made: in expect, save, poll and cleanup"]);
    assert_eq!(code(&Roots::new(), "${request.url}"), Code::V2105);
    assert!(check(&Roots::new().with_call(), "${response.status}").is_ok());
    assert!(check(&Roots::new().with_call(), "${request.url}").is_ok());
}

#[test]
fn bare_response_fields_are_v2105_with_a_certain_fix() {
    for roots in [Roots::new(), Roots::new().with_call()] {
        let diagnostic = *check(&roots, "${headers.etag}").unwrap_err();
        assert_eq!(diagnostic.code, Code::V2105);
        assert_eq!(diagnostic.message, "`headers` is not a root; did you mean `response.headers`?");
        assert_eq!(diagnostic.help, ["write `response.headers`"]);
        let [fix] = &diagnostic.fixes[..] else { panic!("expected one fix") };
        assert!(fix.certain);
        assert_eq!((fix.edits[0].new_text.as_str(), fix.edits[0].range.clone()), ("response.headers", 2..9));
    }
    let spaced = *check(&Roots::new(), "${ status }").unwrap_err();
    assert_eq!(spaced.fixes[0].edits[0].range, 3..9);
    assert!(check(&Roots::new().with_call(), "${response.body}").is_ok());
}

#[test]
fn a_step_named_like_a_response_field_is_that_step() {
    let mut strings = StringTable::new();
    let body = reference("${body.id}", &mut strings);
    let vc_model::Namespace::Step(id) = body.ns else { panic!("expected a step") };
    assert!(Roots::new().with_step(id).check(&body, &strings).is_ok());
}

#[test]
fn bare_response_fields_with_approximate_spans_get_no_fix() {
    let text = "${body.id}";
    let span = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::new(3, 1, 40));
    let mut strings = StringTable::new();
    let template = vc_template::parse(text, ScalarSource { span, style: ScalarStyle::Block }, &mut strings).unwrap();
    let diagnostic = *Roots::new().check(template.single_ref().unwrap(), &strings).unwrap_err();
    assert_eq!((diagnostic.code, diagnostic.primary.span, diagnostic.fixes.len()), (Code::V2105, span, 0));
}

#[test]
fn loop_roots_outside_their_block_are_v2102() {
    let none = Roots::new();
    assert_eq!(
        (code(&none, "${item}"), code(&none, "${iteration.index}"), code(&none, "${example.x}")),
        (Code::V2102, Code::V2102, Code::V2102)
    );
    let diagnostic = *check(&none, "${item}").unwrap_err();
    assert_eq!(diagnostic.help, ["`item` is available only inside forEach"]);
    assert!(check(&Roots::new().in_for_each(), "${item.id}").is_ok());
    assert!(check(&Roots::new().in_for_each(), "${iteration.index}").is_ok());
    assert!(check(&Roots::new().in_loop(), "${iteration.count}").is_ok());
    assert_eq!(code(&Roots::new().in_loop(), "${item}"), Code::V2102);
    assert!(check(&Roots::new().with_examples(), "${example.email}").is_ok());
    assert_eq!(code(&Roots::new().with_examples(), "${iteration}"), Code::V2102);
}

#[test]
fn step_roots_need_that_step() {
    let mut strings = StringTable::new();
    let place = reference("${place.body.id}", &mut strings);
    let other = reference("${login.body}", &mut strings);
    let vc_model::Namespace::Step(id) = place.ns else { panic!("expected a step") };
    let roots = Roots::new().with_call().with_step(id);
    assert!(roots.check(&place, &strings).is_ok());
    let diagnostic = *roots.check(&other, &strings).unwrap_err();
    assert_eq!((diagnostic.code, diagnostic.message.as_str()), (Code::V2105, "`login` is not available here"));
    assert_eq!(diagnostic.help, ["`login` is available only after that action, in the same test or block"]);
    assert_eq!(diagnostic.primary.span.byte_range(), 0..13);
    let unnamed = roots.check(&other, &StringTable::new()).unwrap_err();
    assert_eq!(unnamed.message, "`step` is not available here");
}

#[test]
fn durations_take_a_unit() {
    let ok = |text| vc_template::parse_duration(text, span()).unwrap().get();
    assert_eq!(ok("250ms"), Duration::from_millis(250));
    assert_eq!(ok("2s"), Duration::from_secs(2));
    assert_eq!(ok("3m"), Duration::from_secs(180));
    assert_eq!(ok("12h"), Duration::from_hours(12));
    assert_eq!(ok("0s"), Duration::ZERO);
}

#[test]
fn a_bare_number_gets_a_certain_fix() {
    let diagnostic = *vc_template::parse_duration("500", span()).unwrap_err();
    assert_eq!((diagnostic.code, diagnostic.message.as_str()), (Code::V2301, "invalid duration `500`"));
    let [fix] = &diagnostic.fixes[..] else { panic!("expected one fix") };
    assert!(fix.certain);
    assert_eq!((fix.edits[0].new_text.as_str(), fix.edits[0].range.clone()), ("500ms", 50..55));
}

#[test]
fn bad_durations_have_no_fix() {
    for text in [
        "99999999999999999999",
        "",
        "ms",
        "2 s",
        "2sec",
        "1.5s",
        "-1s",
        "2S",
        "99999999999999999999h",
        "18446744073709551615h",
    ] {
        let diagnostic = *vc_template::parse_duration(text, span()).unwrap_err();
        assert_eq!((diagnostic.code, diagnostic.fixes.len()), (Code::V2301, 0), "{text}");
        assert_eq!(diagnostic.primary.span, span());
    }
}

#[test]
fn sizes_are_binary() {
    let ok = |text| vc_template::parse_size(text, span()).unwrap().bytes();
    assert_eq!((ok("512b"), ok("4kb"), ok("1mb"), ok("2gb")), (512, 4096, 1_048_576, 2_147_483_648));
    for text in ["4", "4k", "4KB", "kb", "4 kb", "18446744073709551615kb"] {
        let diagnostic = *vc_template::parse_size(text, span()).unwrap_err();
        assert_eq!((diagnostic.code, diagnostic.message.clone()), (Code::V2301, format!("invalid size `{text}`")));
    }
}

#[test]
fn rates_are_percentages_or_shares() {
    let ok = |text| vc_template::parse_rate(text, span()).unwrap();
    let share = |value| serde_json::Number::from_f64(value).unwrap();
    assert_eq!([ok("1%"), ok("0.5%"), ok("100%"), ok("0%")], [share(0.01), share(0.005), share(1.0), share(0.0)]);
    assert_eq!([ok("0.01"), ok("1"), ok("0")], [share(0.01), share(1.0), share(0.0)]);
    for text in ["101%", "-1%", "1.5", "50", "%", "1 %", "1e-2", "NaN", "inf", "1%%", ""] {
        let diagnostic = *vc_template::parse_rate(text, span()).unwrap_err();
        assert_eq!((diagnostic.code, diagnostic.message.clone()), (Code::V2301, format!("invalid rate `{text}`")));
        assert_eq!(diagnostic.primary.span, span());
    }
}

#[test]
fn statuses_are_codes_or_classes() {
    let ok = |text| vc_template::parse_status(text, span()).unwrap();
    assert_eq!(
        (ok("201"), ok("100"), ok("599")),
        (StatusPattern::Code(201), StatusPattern::Code(100), StatusPattern::Code(599))
    );
    assert_eq!(
        (ok("1xx"), ok("2xx"), ok("5xx")),
        (StatusPattern::Class(1), StatusPattern::Class(2), StatusPattern::Class(5))
    );
    for text in ["", "99", "099", "600", "2000", "+20", "+99", "+1xx", "0xx", "6xx", "22xx", "xx", "2XX", "+xx", "20x"]
    {
        let diagnostic = *vc_template::parse_status(text, span()).unwrap_err();
        assert_eq!((diagnostic.code, diagnostic.primary.span), (Code::V2303, span()), "{text}");
    }
}
