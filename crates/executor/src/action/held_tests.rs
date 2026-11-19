use std::sync::Arc;

use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use vc_diagnostics::Code;
use vc_model::{
    ActionId, BodySpec, Check, CheckKind, CheckResult, Failure, HttpResponse, Matcher, MatcherArg, MatcherCall, Method,
    Origin, OwnerKey, Protocol, StatusPattern, TemplateValue,
};
use vc_testkit::FakeTransport;

use super::{Held, line};
use crate::fixture::{self, Fixture};
use crate::{ActionExecutors, ActionReport, ActionRequest, Frame, HostLimits, PendingCleanup, PipelineParts};

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

async fn run((fixture, call): (Fixture, ActionId), answer: HttpResponse) -> ActionReport {
    let fake = FakeTransport::scripted(vec![Ok(answer)]);
    let executors = ActionExecutors::standard(PipelineParts::new(Arc::new(fake), HostLimits::spawn(8)));
    let cx = fixture.cx(executors);
    cx.redactor.hide_values([SECRET]);
    let request = ActionRequest { action: call, owner: OwnerKey::Run, cx: Arc::clone(&cx), frame: Frame::default() };
    cx.executors.execute(request).await
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
    let bodies = [("<html>home</html>", "a web page"), ("{\"ok\": true}", "an object"), ("{\"errors\": {}}", "errors")];
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
        Some("The response is not a GraphQL response: the body is not a JSON object with `data` or `errors`")
    );
}

#[tokio::test]
async fn a_non_2xx_body_that_is_not_graphql_is_left_to_the_status_check() {
    let (fixture, create, _) = creating(Protocol::Graphql);
    let report = run((fixture, create), answer(400, "<html>bad</html>")).await;
    assert_eq!(failed_code(&report), Some(Code::V5101.to_string()), "the status check fails, not V5124");
    let (mut fixture, create, _) = creating(Protocol::Graphql);
    let expected = Matcher::Status(vec![StatusPattern::Code(400)]);
    let status = Check {
        kind: CheckKind::Status,
        target: None,
        matcher: expected,
        origin: Origin::Action(create),
        span: fixture::span(),
    };
    fixture.action_mut(create).expect.checks = vec![status];
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
    let unread = Held::new(Err(Failure::new("not GraphQL")));
    assert_eq!(unread.failure(), Some(Failure::new("not GraphQL")));
    assert!(!unread.clean());
    assert_eq!(unread.warnings(&[], true), []);
    let clean = Held::new(Ok(Vec::new()));
    assert_eq!((clean.failure(), clean.clean()), (None, true));
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
async fn a_masked_value_in_an_error_is_hidden_from_the_listing() {
    let mut fixture = Fixture::new();
    let call = fixture.call("/graphql");
    fixture.api_mut(call).protocol = Protocol::Graphql;
    let body = json!({ "data": null, "errors": [{ "message": format!("bad key {SECRET}") }] });
    let report = run((fixture, call), answer(200, &body.to_string())).await;
    let message = &warned(&report)[0].1;
    assert!(message.starts_with("The response has GraphQL errors: bad key ") && !message.contains(SECRET), "{message}");
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
