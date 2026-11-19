use std::sync::Arc;

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_model::{
    ActionId, BodySpec, Check, CheckKind, CheckResult, Event, Failure, HttpResponse, Matcher, MatcherArg, MatcherCall,
    Method, Origin, OwnerKey, Protocol, StatusPattern, TemplateValue,
};
use vc_testkit::FakeTransport;

use super::{Held, line};
use crate::fixture::{self, Fixture};
use crate::{ActionCx, ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PendingCleanup, PipelineParts};

const FORBIDDEN: &str = "FORBIDDEN at user.secretNote: secretNote is not allowed";

/// A value the run masks.
const SECRET: &str = "s3cr3t-key";

/// A body with partial data and one `FORBIDDEN` error on `user.secretNote`.
fn forbidden() -> Value {
    json!({
        "data": { "createUser": { "id": "u-7", "secretNote": null } },
        "errors": [{ "message": "secretNote is not allowed", "path": ["user", "secretNote"],
                     "extensions": { "code": "FORBIDDEN" } }],
    })
}

fn answer(code: u16, body: &str) -> HttpResponse {
    let body = Bytes::from(body.to_owned());
    let headers = HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_static("application/json"))]);
    HttpResponse { status: StatusCode::from_u16(code).unwrap(), headers, wire_size: body.len() as u64, body }
}

/// A `graphql` api's call, saving `userId` from the response, whose cleanup is a call of its own.
fn creating(protocol: Protocol) -> (Fixture, ActionId, ActionId) {
    let mut fixture = Fixture::new();
    let delete = fixture.call("/graphql");
    let create = fixture.creating("/graphql", delete);
    let saves = fixture.vars(&[("userId", "${response.body.data.createUser.id}")]);
    let api = fixture.api_mut(create);
    (api.protocol, api.method, api.body) = (protocol, Method::Post, BodySpec::None);
    api.save.values = saves;
    (fixture, create, delete)
}

async fn run(call: (Fixture, ActionId), answer: HttpResponse) -> ActionReport {
    ran(call, Frame::default(), answer).await.0
}

/// The report of `call` run in `frame` on `answer`, and the run's context.
async fn ran(
    (fixture, call): (Fixture, ActionId),
    frame: Frame,
    answer: HttpResponse,
) -> (ActionReport, Arc<ActionCx>) {
    let fake = FakeTransport::scripted(vec![Ok(answer)]);
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    cx.redactor.hide_values([SECRET]);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame };
    (cx.executors.execute(request).await, cx)
}

/// The step's own `status: code`.
fn status(call: ActionId, code: u16) -> Check {
    let matcher = Matcher::Status(vec![StatusPattern::Code(code)]);
    Check { kind: CheckKind::Status, target: None, matcher, origin: Origin::Action(call), span: fixture::span() }
}

/// Each failed check's kind and code.
fn failed_checks(report: &ActionReport) -> Vec<(CheckKind, Option<String>)> {
    report.checks.iter().filter(|check| !check.passed).map(|check| (check.kind, check.code.clone())).collect()
}

/// Each warning's code and message.
fn warned(report: &ActionReport) -> Vec<(Option<String>, String)> {
    report.warnings.iter().map(|warning| (warning.code.clone(), warning.message.clone())).collect()
}

fn codes(report: &ActionReport) -> Vec<String> {
    report.warnings.iter().filter_map(|warning| warning.code.clone()).collect()
}

fn failed_code(report: &ActionReport) -> Option<String> {
    report.outcome.failure().and_then(|failure| failure.code.clone())
}

/// `errors: none`, as the empty list form the compiler lowers it to (D362).
fn no_errors(call: ActionId) -> Check {
    let none = MatcherCall { name: "entries".to_owned(), arg: MatcherArg::Matchers(Vec::new()), span: fixture::span() };
    let matcher = Matcher::All(vec![none]);
    Check { kind: CheckKind::Errors, target: None, matcher, origin: Origin::Action(call), span: fixture::span() }
}

#[tokio::test]
async fn a_200_with_errors_passes_lists_them_and_holds_back_the_default_save_and_the_cleanup() {
    let (fixture, create, _) = creating(Protocol::Graphql);
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(report.outcome.failure(), None, "errors pass by default (D345)");
    assert_eq!((report.saved.len(), report.cleanups.len()), (0, 0), "no save, no cleanup, no V5111");
    let listed = format!("The response has GraphQL errors: {FORBIDDEN}");
    assert_eq!(warned(&report)[0], (Some(Code::V5128.to_string()), listed));
    assert_eq!(codes(&report), [Code::V5128.to_string(), Code::V5129.to_string()]);
    assert!(report.warnings.iter().all(|warning| warning.action == Some(create)), "each is at the call");
}

#[tokio::test]
async fn an_explicit_save_on_saves_on_errors_but_the_cleanup_stays_held_back() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.api_mut(create).save_on_written = true;
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(report.saved.get("userId"), Some(&json!("u-7")), "an explicit saveOn is status only");
    assert_eq!(report.cleanups, vec![]);
    assert_eq!(codes(&report), [Code::V5128.to_string(), Code::V5129.to_string()], "the cleanup was held back");
}

#[tokio::test]
async fn a_response_without_errors_saves_and_registers_its_cleanup_with_no_warning() {
    for body in [
        json!({ "data": { "createUser": { "id": "u-7" } } }),
        json!({ "data": { "createUser": { "id": "u-7" } }, "errors": [] }),
    ] {
        let (fixture, create, delete) = creating(Protocol::Graphql);
        let report = run((fixture, create), answer(200, &body.to_string())).await;
        assert_eq!(report.saved.get("userId"), Some(&json!("u-7")), "{body}");
        assert_eq!(report.cleanups, vec![PendingCleanup::new(delete)], "{body}");
        assert_eq!(warned(&report), [], "{body}");
    }
}

#[tokio::test]
async fn an_api_with_neither_save_nor_cleanup_only_lists_the_errors() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/graphql");
    fixture.api_mut(call).protocol = Protocol::Graphql;
    let report = run((fixture, call), answer(200, &forbidden().to_string())).await;
    assert_eq!(codes(&report), [Code::V5128.to_string()], "nothing was held back");
}

#[tokio::test]
async fn a_status_save_on_does_not_match_is_no_save_held_back() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.api_mut(create).save_on = Matcher::Status(vec![StatusPattern::Code(201)]);
    fixture.api_mut(create).cleanup = None;
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(codes(&report), [Code::V5128.to_string()], "the save would not have run anyway");
}

#[tokio::test]
async fn a_failed_errors_check_is_the_failure_and_the_errors_are_not_listed_again() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.action_mut(create).expect.checks = vec![no_errors(create)];
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(failed_code(&report), Some(Code::V5123.to_string()));
    assert_eq!(codes(&report), [Code::V5129.to_string()]);
}

#[tokio::test]
async fn a_2xx_body_that_is_not_graphql_fails_with_v5124_and_registers_no_cleanup() {
    let bodies = [
        ("<html>home</html>", "a web page"),
        ("{\"ok\": true}", "an object"),
        ("{\"errors\": {}}", "errors"),
        ("{\"errors\": null}", "a null errors (D363)"),
    ];
    for (body, what) in bodies {
        let (fixture, create, _) = creating(Protocol::Graphql);
        let report = run((fixture, create), answer(200, body)).await;
        assert_eq!(failed_code(&report), Some(Code::V5124.to_string()), "{what}");
        assert_eq!((report.saved.len(), report.cleanups.len(), report.warnings.len()), (0, 0, 0), "{what}");
    }
    let (fixture, create, _) = creating(Protocol::Graphql);
    let report = run((fixture, create), answer(204, "")).await;
    let failure = report.outcome.failure().map(|failure| failure.message.clone());
    assert_eq!(
        failure.as_deref(),
        Some("errors: The response is not a GraphQL response: the body is not a JSON object with `data` or `errors`")
    );
}

#[tokio::test]
async fn a_non_2xx_body_that_is_not_graphql_is_left_to_the_status_check() {
    let (fixture, create, _) = creating(Protocol::Graphql);
    let report = run((fixture, create), answer(400, "<html>bad</html>")).await;
    assert_eq!(failed_code(&report), Some(Code::V5101.to_string()), "the status check fails, not V5124");
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.action_mut(create).expect.checks = vec![status(create, 400)];
    let report = run((fixture, create), answer(400, "<html>bad</html>")).await;
    assert_eq!((report.outcome.failure(), report.warnings.len()), (None, 0), "a 400 it expects passes");
}

#[tokio::test]
async fn an_http_api_ignores_an_errors_key() {
    let (fixture, create, delete) = creating(Protocol::Http);
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(report.saved.get("userId"), Some(&json!("u-7")));
    assert_eq!((report.cleanups, report.warnings.len()), (vec![PendingCleanup::new(delete)], 0));
    let (mut fixture, create, _) = creating(Protocol::Http);
    fixture.api_mut(create).save.values.clear();
    assert_eq!(run((fixture, create), answer(200, "<html/>")).await.outcome.failure(), None, "no V5124 on http");
}

#[test]
fn each_error_is_listed_as_code_at_path_message_leaving_out_what_it_lacks() {
    let cases = [
        (json!({"message": "no", "path": "user.note", "code": "FORBIDDEN"}), "FORBIDDEN at user.note: no"),
        (json!({"message": "no", "path": "user.note"}), "at user.note: no"),
        (json!({"message": "no", "code": 42}), "42: no"),
        (json!({"message": "no"}), "no"),
        (json!({"code": "TIMEOUT"}), "TIMEOUT"),
        (json!({}), "an error with no message"),
        (json!({"message": "no", "path": null, "code": null}), "no"),
        (json!({"message": null, "code": "TIMEOUT"}), "TIMEOUT"),
    ];
    for (error, said) in cases {
        assert_eq!(line(&error), said, "{error}");
    }
}

#[test]
fn the_listing_joins_the_errors_and_any_errors_check_silences_it() {
    let held = Held::new(Ok(vec![json!({"message": "a"}), json!({"code": "B", "message": "b"})]));
    let status = CheckResult {
        kind: CheckKind::Status,
        target: None,
        origin: Origin::Action(ActionId::new(0)),
        span: fixture::span(),
        passed: false,
        expected: Value::Null,
        actual: None,
        diff: None,
        error: None,
        code: None,
    };
    let listed = Failure::new("The response has GraphQL errors: a; B: b").with_code(Code::V5128.to_string());
    assert_eq!(held.warnings(std::slice::from_ref(&status), false), [listed], "a status check is no errors check");
    for passed in [true, false] {
        let errors = CheckResult { kind: CheckKind::Errors, passed, ..status.clone() };
        assert_eq!(held.warnings(&[errors], true).len(), 1, "D364: only V5129 once errors are checked ({passed})");
    }
}

#[test]
fn an_unread_response_fails_holds_everything_back_and_warns_nothing() {
    let unread = Held::new(Err(Failure::new("not GraphQL").with_code(Code::V5124.to_string())));
    let at = (Origin::Action(ActionId::new(3)), fixture::span());
    let reported = unread.reported(Vec::new(), at);
    let failed = |check: &CheckResult| (check.passed, check.origin, check.error.clone(), check.code.clone());
    let expected = (false, at.0, Some("not GraphQL".to_owned()), Some(Code::V5124.to_string()));
    assert_eq!(reported.iter().map(failed).collect::<Vec<_>>(), [expected], "the call's own, at its span");
    assert!(!unread.clean());
    assert_eq!(unread.warnings(&reported, true), []);
    let clean = Held::new(Ok(Vec::new()));
    assert_eq!((clean.reported(Vec::new(), at), clean.clean()), (vec![], true));
}

#[tokio::test]
async fn an_explicit_save_on_with_no_cleanup_holds_nothing_back() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    (fixture.api_mut(create).save_on_written, fixture.api_mut(create).cleanup) = (true, None);
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(report.saved.get("userId"), Some(&json!("u-7")));
    assert_eq!(codes(&report), [Code::V5128.to_string()], "the save ran, so no V5129");
}

#[tokio::test]
async fn a_masked_value_in_an_error_is_hidden_from_the_listing_at_the_source_and_on_its_way_out() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/graphql");
    fixture.api_mut(call).protocol = Protocol::Graphql;
    let body = json!({ "data": null, "errors": [{ "message": format!("bad key {SECRET}") }] });
    let (mut report, cx) = ran((fixture, call), Frame::default(), answer(200, &body.to_string())).await;
    let warning = report.warnings.remove(0);
    assert!(!warning.message.contains(SECRET), "masked at the source: {}", warning.message);
    let Event::Warned { warning, .. } =
        cx.redactor.event(Event::Warned { action: call, owner: OwnerKey::Run, warning }).await
    else {
        panic!("a warning stays one")
    };
    let message = warning.message;
    assert!(message.starts_with("The response has GraphQL errors: bad key ") && !message.contains(SECRET), "{message}");
}

#[tokio::test]
async fn a_failed_check_already_holds_the_save_back_so_no_v5129_for_it() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.api_mut(create).cleanup = None;
    fixture.action_mut(create).expect.checks = vec![status(create, 201)];
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(failed_code(&report), Some(Code::V5101.to_string()), "status: 201 fails");
    assert_eq!(codes(&report), [Code::V5128.to_string()], "the save was not going to run");
}

#[tokio::test]
async fn v5124_is_the_first_failed_check_and_shows_with_the_others() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.action_mut(create).expect.checks = vec![status(create, 201)];
    let report = run((fixture, create), answer(200, "<html/>")).await;
    let failed = failed_checks(&report);
    let (v5124, v5101) = (Some(Code::V5124.to_string()), Some(Code::V5101.to_string()));
    assert_eq!(failed, [(CheckKind::Errors, v5124.clone()), (CheckKind::Status, v5101)]);
    let unread = &report.checks[0];
    assert_eq!(unread.origin, Origin::Action(create), "the call's own check");
    assert!(unread.error.as_deref().unwrap().starts_with("The response is not"));
    assert_eq!(failed_code(&report), v5124, "the verdict is the first failed check");
}

#[tokio::test]
async fn a_judged_errors_check_becomes_v5124_so_it_is_reported_once() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    let checks = vec![status(create, 200), no_errors(create)];
    fixture.action_mut(create).expect.checks = checks.clone();
    let report = run((fixture, create), answer(200, "<html/>")).await;
    let errors: Vec<_> = report.checks.iter().filter(|check| check.kind == CheckKind::Errors).collect();
    assert_eq!(errors.len(), 1, "one result per cause");
    assert_eq!((errors[0].code.clone(), errors[0].passed), (Some(Code::V5124.to_string()), false));
    assert_eq!(report.checks[0].kind, CheckKind::Errors, "first");
    assert_eq!(failed_code(&report), Some(Code::V5124.to_string()));
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    fixture.action_mut(create).expect.checks = checks;
    let report = run((fixture, create), answer(200, "{\"errors\": null}")).await;
    let failed = (report.checks[0].passed, failed_code(&report));
    assert_eq!(failed, (false, Some(Code::V5124.to_string())), "errors: none holds on it, V5124 still fails");
}

#[tokio::test]
async fn a_load_iteration_judging_only_the_apis_checks_lists_the_errors_the_skipped_step_check_would_assert() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/graphql");
    fixture.api_mut(call).protocol = Protocol::Graphql;
    fixture.action_mut(call).expect.checks = vec![no_errors(call)];
    let frame = Frame { api_checks_only: true, ..Frame::default() };
    let (report, _) = ran((fixture, call), frame, answer(200, &forbidden().to_string())).await;
    assert_eq!((report.outcome.failure(), failed_checks(&report)), (None, vec![]), "checks: api skips it");
    assert_eq!(codes(&report), [Code::V5128.to_string()], "D365: only a check that ran silences V5128");
}

#[tokio::test]
async fn a_passing_errors_any_asserts_the_errors_so_they_are_not_listed() {
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    let any = MatcherCall {
        name: "empty".to_owned(),
        arg: MatcherArg::Value(TemplateValue::Bool(false)),
        span: fixture::span(),
    };
    let check = Check { matcher: Matcher::All(vec![any]), ..no_errors(create) };
    fixture.action_mut(create).expect.checks = vec![check];
    let report = run((fixture, create), answer(200, &forbidden().to_string())).await;
    assert_eq!(report.outcome.failure(), None, "errors: any holds");
    assert_eq!(codes(&report), [Code::V5129.to_string()], "D364: asserted errors are not listed");
}
